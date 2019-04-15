use super::IndexPackage;
use crate::{
    lock::Lock,
    util::{crate_walker, pkg_path},
};
use failure::{Error, ResultExt};
use semver::VersionReq;
use std::{fs, iter::Iterator, path::Path};

/// List entries in the index.
///
/// This will list all entries for a particular package in the index. If the
/// version is not specified, all versions are returned. The version supports
/// semver requirement syntax.
pub fn list(
    index: impl AsRef<Path>,
    pkg_name: &str,
    version_req: Option<&str>,
) -> Result<Vec<IndexPackage>, Error> {
    let index = index.as_ref();
    let lock = Lock::new_shared(index)?;
    let version_req = if let Some(version) = version_req {
        Some(VersionReq::parse(version)?)
    } else {
        None
    };
    let res = _list(index, pkg_name, version_req.as_ref())?;
    drop(lock);
    Ok(res)
}

/// List all entries for all packages in the index.
///
/// If `pkg_name` is set, only list the given package.
/// If `version_req` is set, filters with the given semver requirement.
/// The given callback will be called for each version.
pub fn list_all(
    index: impl AsRef<Path>,
    pkg_name: Option<&str>,
    version_req: Option<&str>,
    mut cb: impl FnMut(IndexPackage),
) -> Result<(), Error> {
    let index = index.as_ref();
    let lock = Lock::new_shared(index)?;
    let version_req = if let Some(version_req) = version_req {
        Some(VersionReq::parse(version_req)?)
    } else {
        None
    };
    if let Some(pkg_name) = pkg_name {
        for pkg in _list(index, pkg_name, version_req.as_ref())? {
            cb(pkg);
        }
    } else {
        for entry in crate_walker(index) {
            let entry = entry?;
            for pkg in _list(
                index,
                entry.file_name().to_str().unwrap(),
                version_req.as_ref(),
            )? {
                cb(pkg);
            }
        }
    };
    drop(lock);
    Ok(())
}

pub(crate) fn _list(
    index: &Path,
    pkg_name: &str,
    version_req: Option<&VersionReq>,
) -> Result<Vec<IndexPackage>, Error> {
    let repo_path = pkg_path(pkg_name);
    let path = index.join(repo_path);
    if !path.exists() {
        return Ok(vec![]);
    }
    let contents = fs::read_to_string(&path)
        .with_context(|_| format!("Failed to read `{}`.", path.display()))?;
    contents
        .lines()
        .map(|line| {
            Ok(serde_json::from_str(line).with_context(|_| {
                format!("Could not deserialize `{}` line:\n{}", path.display(), line)
            })?)
        })
        .filter(|index_pkg: &Result<IndexPackage, Error>| -> bool {
            if let Some(version_req) = &version_req {
                if let Ok(index_pkg) = index_pkg {
                    version_req.matches(&index_pkg.vers)
                } else {
                    true
                }
            } else {
                true
            }
        })
        .collect::<Result<Vec<IndexPackage>, Error>>()
}
