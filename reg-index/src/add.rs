use crate::{
    list::_list,
    lock::Lock,
    metadata::{metadata_reg, MetaInfo},
    util::{extract_crate, pkg_path, signature},
    IndexPackage,
};
use failure::{bail, Error, ResultExt};
use git2;
use semver::VersionReq;
use std::{fs, io::Write, path::Path};

/// Add a new entry to the index.
///
/// The `index_url` should be the public URL that users use to access the
/// index this package will be added to. The `index_path` should be the
/// filesystem path to the index.
///
/// If the `manifest_path` is not given, it will search the current directory
/// for the manifest.
///
/// This will call `cargo package` to generate a `.crate` file. The
/// `package_args` will be given as-is to the `cargo package` command. See
/// [`add_from_crate`] for a variant of this function that takes a
/// pre-existing `.crate` file.
///
/// `upload` is an optional path to a directory to copy the `.crate` file to
/// after it has been added to the index. It may contain `{crate}` and
/// `{version}` markers.
///
/// This only performs minimal validity checks on the crate. Callers should
/// consider adding more validation before calling. For example, placing
/// restrictions on the crate name format, checking dependencies with `registry`
/// set, limit category names, etc. See the [crates.io code] for examples
/// of the many checks it applies.
///
/// [`add_from_crate`]: fn.add_from_crate.html
/// [crates.io code]: https://github.com/rust-lang/crates.io
pub fn add(
    index_path: impl AsRef<Path>,
    index_url: &str,
    manifest_path: Option<&Path>,
    upload: Option<&str>,
    package_args: Option<&Vec<String>>,
) -> Result<IndexPackage, Error> {
    add_reg(
        index_path,
        index_url,
        manifest_path,
        None,
        upload,
        package_args,
    )
}

pub(crate) fn add_reg(
    index_path: impl AsRef<Path>,
    index_url: &str,
    manifest_path: Option<&Path>,
    crate_path: Option<&Path>,
    upload: Option<&str>,
    package_args: Option<&Vec<String>>,
) -> Result<IndexPackage, Error> {
    let MetaInfo {
        index_pkg,
        crate_path,
    } = metadata_reg(index_url, manifest_path, crate_path, package_args)?;
    let mut meta_json = serde_json::to_string(&index_pkg)?;
    meta_json.push('\n');
    // Add to git repo.
    let index_path = index_path.as_ref();
    let repo = git2::Repository::open(index_path)
        .with_context(|_| format!("Could not open index at `{}`.", index_path.display()))?;
    let lock = Lock::new_exclusive(index_path)?;
    let matching_pkgs = _list(
        index_path,
        &index_pkg.name,
        Some(&VersionReq::exact(&index_pkg.vers)),
    )?;
    if !matching_pkgs.is_empty() {
        bail!(
            "Package `{}` version `{}` is already in the index.",
            index_pkg.name,
            index_pkg.vers
        );
    }
    for dep in &index_pkg.deps {
        if dep.registry.is_none() {
            let dep_name = dep.package.as_ref().unwrap_or(&dep.name);
            let matching_deps = _list(index_path, dep_name, Some(&dep.req))?;
            if matching_deps.is_empty() {
                bail!(
                    "Package `{}` dependency `{}:{}` not found in index.",
                    index_pkg.name,
                    dep_name,
                    dep.req
                );
            }
        }
    }
    let repo_path = pkg_path(&index_pkg.name);
    let path = index_path.join(&repo_path);
    let dir_path = path.parent().unwrap();
    fs::create_dir_all(&dir_path)
        .with_context(|_| format!("Failed to create directory `{}`.", dir_path.display()))?;
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|_| format!("Failed to create or open `{}`.", path.display()))?;
    f.write_all(meta_json.as_bytes())
        .with_context(|_| format!("Failed to write json entry at `{}`.", path.display()))?;
    let msg = format!("Updating crate '{}#{}'", index_pkg.name, index_pkg.vers);
    // Upload.
    if let Some(upload) = upload {
        let replaced = upload
            .replace("{crate}", &index_pkg.name)
            .replace("{version}", &index_pkg.vers.to_string());
        let upload = Path::new(&replaced);
        fs::create_dir_all(upload)?;
        fs::copy(&crate_path, upload.join(&crate_path.file_name().unwrap()))?;
    }
    git_add(&repo, &repo_path, &msg).with_context(|_| "Failed to add to git repo.")?;
    drop(lock);
    Ok(index_pkg)
}

/// Add and commit a file to a git repo.
pub(crate) fn git_add(repo: &git2::Repository, path: &Path, msg: &str) -> Result<(), Error> {
    let mut index = repo.index()?;
    index.add_path(path)?;
    index.write()?;
    let id = index.write_tree()?;
    let tree = repo.find_tree(id)?;
    let head = repo.head()?;
    let parent = repo.find_commit(head.target().unwrap())?;
    let sig = signature(&repo)?;
    repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &[&parent])?;
    Ok(())
}

/// Add a new entry to the index.
///
/// This will add an entry based on the contents of a `.crate` file. See
/// [`add`] for a variant that takes a path to a `Cargo.toml` manifest, and
/// for more details on how this works.
///
/// [`add`]: fn.add.html
pub fn add_from_crate(
    index_path: impl AsRef<Path>,
    index_url: &str,
    crate_path: impl AsRef<Path>,
    upload: Option<&str>,
) -> Result<IndexPackage, Error> {
    let crate_path = crate_path.as_ref();
    let (_tmp_dir, pkg_path) = extract_crate(crate_path)?;
    let manifest_path = pkg_path.join("Cargo.toml");
    add_reg(
        index_path,
        index_url,
        Some(&manifest_path),
        Some(crate_path),
        upload,
        None,
    )
}
