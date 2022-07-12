use crate::{
    add::git_add,
    lock::Lock,
    util::{pkg_path, vers_eq},
    IndexPackage,
};
use failure::{bail, Error, ResultExt};
use semver::Version;
use std::{fs, path::Path};

/// Yank a version in the index.
///
/// This sets the `yank` field to true. This will fail if it is already set.
pub fn yank(index: impl AsRef<Path>, pkg_name: &str, version: &str) -> Result<(), Error> {
    set_yank(index, pkg_name, version, true)
}

/// Unyank a version in the index.
///
/// This sets the `yank` field to false. This will fail if it is not yanked.
pub fn unyank(index: impl AsRef<Path>, pkg_name: &str, version: &str) -> Result<(), Error> {
    set_yank(index, pkg_name, version, false)
}

/// Set the `yank` value of a package in the index.
///
/// This will fail if it is already set to the given value.
pub fn set_yank(
    index: impl AsRef<Path>,
    pkg_name: &str,
    version: &str,
    yank: bool,
) -> Result<(), Error> {
    let version = Version::parse(version)?;
    let index = index.as_ref();
    let repo = git2::Repository::open(index)
        .with_context(|_| format!("Could not open index at `{}`.", index.display()))?;
    let lock = Lock::new_exclusive(index)?;
    let repo_path = pkg_path(pkg_name);
    let path = index.join(&repo_path);
    if !path.exists() {
        bail!("Package `{}` is not in the index.", pkg_name);
    }
    let contents = fs::read_to_string(&path)
        .with_context(|_| format!("Failed to read `{}`.", path.display()))?;
    let (lines, matches): (Vec<String>, Vec<u32>) = contents
        .lines()
        .map(|line| {
            let mut pkg: IndexPackage = serde_json::from_str(line).with_context(|_| {
                format!(
                    "Failed to deserialize line in `{}`:\n{}",
                    path.display(),
                    line
                )
            })?;
            if vers_eq(&pkg.vers, &version) {
                if pkg.yanked == yank {
                    if yank {
                        bail!("`{}:{}` is already yanked!", pkg_name, version);
                    } else {
                        bail!("`{}:{}` is not yanked!", pkg_name, version);
                    }
                }
                pkg.yanked = yank;
                let mut new_line = serde_json::to_string(&pkg)?;
                new_line.push('\n');
                Ok((new_line, 1))
            } else {
                let mut new_line = line.to_string();
                new_line.push('\n');
                Ok((new_line, 0))
            }
        })
        .collect::<Result<Vec<(String, u32)>, Error>>()?
        .into_iter()
        .unzip();
    match matches.iter().sum() {
        0 => bail!(
            "Version `{}` for package `{}` not found.",
            version,
            pkg_name
        ),
        1 => {}
        _ => bail!(
            "Version `{}` for package `{}` found multiple times, is the index corrupt?",
            version,
            pkg_name
        ),
    }
    fs::write(&path, lines.join(""))
        .with_context(|_| format!("Failed to write `{}`.", path.display()))?;
    let what = if yank { "Yanking" } else { "Unyanking" };
    git_add(
        &repo,
        &repo_path,
        &format!("{} crate `{}:{}`", what, pkg_name, version),
    )?;
    drop(lock);
    Ok(())
}
