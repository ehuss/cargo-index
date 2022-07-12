use crate::{
    util::{cargo_package, cksum, extract_crate},
    IndexDependency, IndexPackage,
};
use failure::{bail, format_err, Error, ResultExt};
use same_file::is_same_file;
use std::{
    env,
    path::{Path, PathBuf},
};
use url::Url;

pub(crate) struct MetaInfo {
    pub(crate) index_pkg: IndexPackage,
    pub(crate) crate_path: PathBuf,
}

/// Get the metadata for a package *before* publishing it.
///
/// This will get the metadata directly from a `.crate` file. See [`metadata`]
/// for a variant of this function that takes a path to a `Cargo.toml`
/// manifest, and for more details on how this works.
///
/// [`metadata`]: fn.metadata.html
pub fn metadata_from_crate(
    index_url: &str,
    crate_path: impl AsRef<Path>,
) -> Result<IndexPackage, Error> {
    let crate_path = crate_path.as_ref();
    let (_tmp_dir, pkg_path) = extract_crate(crate_path)?;
    Ok(metadata_reg(
        index_url,
        Some(&pkg_path.join("Cargo.toml")),
        Some(crate_path),
        None,
    )?
    .index_pkg)
}

/// Get the metadata for a package *before* publishing it.
///
/// If the `manifest_path` is not given, it will search the current directory
/// for the manifest.
///
/// This will call `cargo package` to generate a `.crate` file. The
/// `package_args` will be given as-is to the `cargo package` command. See
/// [`metadata_from_crate`] for a variant of this function that takes a
/// pre-existing `.crate` file.
///
/// The `index_url` should be the public URL that users use to access the
/// index this package will be added to.
///
/// [`metadata_from_crate`]: fn.metadata_from_crate.html
pub fn metadata(
    index_url: &str,
    manifest_path: Option<&Path>,
    package_args: Option<&Vec<String>>,
) -> Result<IndexPackage, Error> {
    Ok(metadata_reg(index_url, manifest_path, None, package_args)?.index_pkg)
}

pub(crate) fn metadata_reg(
    index_url: &str,
    manifest_path: Option<&Path>,
    crate_path: Option<&Path>,
    package_args: Option<&Vec<String>>,
) -> Result<MetaInfo, Error> {
    let mut cmd = cargo_metadata::MetadataCommand::new();
    if let Some(path) = manifest_path {
        if let Some(parent) = path.parent() {
            cmd.current_dir(parent);
        } else {
            cmd.manifest_path(path);
        }
    }
    let metadata = cmd
        .exec()
        .map_err(|e| format_err!("{}", e))
        .with_context(|_| match manifest_path {
            Some(path) => format_err!("Failed to read manifest at `{}`.", path.display()),
            None => format_err!("Failed to read manifest from current directory."),
        })?;
    // Pick the package that matches this manifest path.
    let cwd = env::current_dir()?;
    let actual_manifest_path = match manifest_path {
        Some(path) => path.to_path_buf(),
        None => cwd
            .ancestors()
            .map(|p| p.join("Cargo.toml"))
            .find(|p| p.exists())
            .ok_or_else(|| {
                format_err!(
                    "Could not find `Cargo.toml` in `{}` or any parent.",
                    cwd.display()
                )
            })?,
    };
    let pkg = metadata
        .packages
        .iter()
        .find(|p| is_same_file(&p.manifest_path, &actual_manifest_path).unwrap_or(false))
        .ok_or_else(|| {
            format_err!(
                "Could not find package at `{}`.",
                actual_manifest_path.display()
            )
        })?;

    // Check the .crate file.
    let crate_path = match crate_path {
        Some(path) => {
            if !path.exists() {
                bail!("Crate file not found at `{}`", path.display());
            }
            path.to_path_buf()
        }
        None => cargo_package(
            &actual_manifest_path,
            metadata.target_directory.as_ref(),
            pkg,
            package_args,
        )?,
    };

    let cksum = cksum(&crate_path)?;
    // Create the metadata.
    let deps: Vec<IndexDependency> = pkg
        .dependencies
        .iter()
        .map(|dep| {
            let (name, package) = match &dep.rename {
                Some(new_name) => (new_name.clone(), Some(dep.name.clone())),
                None => (dep.name.clone(), None),
            };
            println!("dep ={:?} index_url={:?}", dep.registry, index_url);
            let registry = dep
                .registry
                .as_ref()
                .map(|s| s.as_ref())
                .or_else(|| {
                    // None means it is from crates.io.
                    Some("https://github.com/rust-lang/crates.io-index")
                })
                .and_then(|r| {
                    // In the index, None means it is from the same registry.
                    if r == index_url {
                        None
                    } else {
                        Some(Url::parse(r).unwrap())
                    }
                });
            IndexDependency {
                name,
                req: dep.req.clone(),
                features: dep.features.clone(),
                optional: dep.optional,
                default_features: dep.uses_default_features,
                target: dep.target.as_ref().map(|t| format!("{}", t)),
                kind: dep.kind,
                registry,
                package,
                __nonexhaustive: (),
            }
        })
        .collect();
    let index_pkg = IndexPackage {
        name: pkg.name.clone(),
        vers: pkg.version.clone(),
        deps,
        features: pkg.features.clone().into_iter().collect(),
        features2: None,
        cksum,
        yanked: false,
        links: pkg.links.clone(),
        v: None,
        __nonexhaustive: (),
    };
    let info = MetaInfo {
        index_pkg,
        crate_path,
    };
    Ok(info)
}
