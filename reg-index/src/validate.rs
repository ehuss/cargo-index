use crate::{
    load_config,
    lock::Lock,
    util::{cksum, crate_walker},
    IndexPackage,
};
use anyhow::{bail, format_err, Context, Error};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};

/// Validate an index.
///
/// Errors are displayed on stdout. Returns an error if any problems are
/// found. `crates` is an optional path to a directory that contains `.crate`
/// files to verify checksums. Supports `{crate}` and `{version}` markers.
pub fn validate(index: impl AsRef<Path>, crates: Option<&str>) -> Result<(), Error> {
    let index = index.as_ref();
    if !index.exists() {
        bail!("Index does not exist at `{}`.", index.display());
    }
    let lock = Lock::new_exclusive(index)?;
    load_config(index)?;
    let mut crate_map = HashMap::new();
    let mut found_err = _validate(&mut crate_map, index, crates)?;
    found_err |= _validate_deps(&crate_map)?;
    drop(lock);
    if found_err {
        bail!("Found at least one error in the index.");
    } else {
        Ok(())
    }
}

fn _validate(
    crate_map: &mut HashMap<String, Vec<IndexPackage>>,
    index: &Path,
    crates: Option<&str>,
) -> Result<bool, Error> {
    let mut found_err = false;
    macro_rules! t {
        ($e:expr) => {
            match $e {
                Ok(e) => e,
                Err(e) => {
                    found_err = true;
                    println!("{}", e);
                    continue;
                }
            }
        };
    }
    macro_rules! err {
        ($fmt:expr, $($arg:tt)+) => {
            println!($fmt, $($arg)+);
            found_err = true;
        };
    }
    for entry in crate_walker(index) {
        let entry = entry?;
        let file_name = entry.file_name();
        let path = entry.path();
        let name = t!(file_name.to_str().ok_or_else(|| format_err!(
            "Expected UTF-8 file name, got `{}` at `{}`.",
            entry.file_name().to_string_lossy(),
            path.display()
        )));
        let parts = path.strip_prefix(index).unwrap();
        let correct = match name.len() {
            1 => Path::new("1").join(name) == parts,
            2 => Path::new("2").join(name) == parts,
            3 => Path::new("3").join(&name[..1]).join(name) == parts,
            _ => Path::new(&name[0..2]).join(&name[2..4]).join(name) == parts,
        };
        if !correct {
            err!("File `{}` is not in the correct location.", path.display());
            continue;
        }
        let contents = t!(fs::read_to_string(&path)
            .with_context(|| format!("Failed to read `{}`.", path.display())));
        let mut seen = HashSet::new();
        for line in contents.lines() {
            let pkg: IndexPackage = t!(serde_json::from_str(line).with_context(|| format!(
                "Could not deserialize `{}` line:\n{}",
                path.display(),
                line
            )));
            let all_vers = crate_map.entry(pkg.name.clone()).or_default();
            all_vers.push(pkg.clone());
            if !seen.insert(pkg.vers.to_string()) {
                err!(
                    "Version `{}` appears multiple times in `{}`.",
                    pkg.vers,
                    pkg.name
                );
            }
            t!(validate_package_name(&pkg.name, "package name"));
            if pkg.name.to_lowercase() != file_name.to_str().unwrap() {
                err!(
                    "Package `{}:{}` does not match file name `{}`.",
                    pkg.name,
                    pkg.vers,
                    path.display()
                );
            }
            // Features could potentially have significant validation.
            // See `build_feature_map` in Cargo.
            for dep in pkg.deps {
                t!(validate_package_name(
                    &dep.name,
                    &format!("dependency of `{}:{}`", pkg.name, pkg.vers),
                ));
            }
            if let Some(crates) = crates {
                let replaced = crates
                    .replace("{crate}", &pkg.name)
                    .replace("{version}", &pkg.vers.to_string());
                let crate_path =
                    Path::new(&replaced).join(format!("{}-{}.crate", pkg.name, pkg.vers));
                if !crate_path.exists() {
                    err!("Could not find crate file: {}", crate_path.display());
                    continue;
                }
                let cksum = t!(cksum(&crate_path));
                if pkg.cksum != cksum {
                    err!(
                        "Checksum did not match for package `{}:{}`:\nindex: {}\nactual:{}",
                        pkg.name,
                        pkg.vers,
                        pkg.cksum,
                        cksum
                    );
                }
            }
        }
    }
    Ok(found_err)
}

fn _validate_deps(crate_map: &HashMap<String, Vec<IndexPackage>>) -> Result<bool, Error> {
    let mut found_err = false;
    for versions in crate_map.values() {
        for pkg in versions {
            for dep in &pkg.deps {
                if dep.registry.is_none() {
                    // Check RegDep exists (if same reg).
                    let dep_name = dep.package.as_ref().unwrap_or(&dep.name);
                    let dep_versions = crate_map.get(dep_name);
                    match dep_versions {
                        Some(dep_versions) => {
                            if !dep_versions
                                .iter()
                                .any(|dep_version| dep.req.matches(&dep_version.vers))
                            {
                                println!("Could not find dependency `{}` matching requirement `{}` from package `{}:{}`.",
                                dep_name, dep.req, pkg.name, pkg.vers);
                                found_err = true;
                            }
                        }
                        None => {
                            println!(
                                "Could not find dependency name `{}` from package `{}:{}`.",
                                dep_name, pkg.name, pkg.vers
                            );
                            found_err = true;
                        }
                    }
                }
            }
        }
    }
    Ok(found_err)
}

fn validate_package_name(name: &str, what: &str) -> Result<(), Error> {
    if let Some(ch) = name
        .chars()
        .find(|ch| !ch.is_alphanumeric() && *ch != '_' && *ch != '-')
    {
        bail!("Invalid character `{}` in {}: `{}`", ch, what, name);
    }
    Ok(())
}
