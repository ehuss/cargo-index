use failure::{bail, Error, ResultExt};
use git2;
use semver::Version;
use sha2::Digest;
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
};
use walkdir::{DirEntry, WalkDir};

pub(crate) fn signature(repo: &git2::Repository) -> Result<git2::Signature, Error> {
    Ok(repo
        .signature()
        .or_else(|e| {
            let name = env::var("GIT_AUTHOR_NAME").or_else(|_| env::var("GIT_COMMITTER_NAME"));
            let email = env::var("GIT_AUTHOR_EMAIL").or_else(|_| env::var("GIT_COMMITTER_EMAIL"));
            if name.is_err() || email.is_err() {
                return Err(e);
            }
            git2::Signature::now(&name.unwrap(), &email.unwrap())
        })
        .with_context(|_| {
            "Could not determine git username/email for signature. \
             Be sure to set `user.name` and `user.email` in gitconfig."
        })?)
}

/// Call `cargo package` to generate a `.crate` file.
pub(crate) fn cargo_package(
    manifest_path: &Path,
    target_dir: &Path,
    pkg: &cargo_metadata::Package,
    package_args: Option<&Vec<String>>,
) -> Result<PathBuf, Error> {
    let mut cmd = Command::new("cargo");
    cmd.arg("package")
        .arg("--manifest-path")
        .arg(manifest_path)
        .current_dir(manifest_path.parent().unwrap());
    if let Some(args) = package_args {
        cmd.args(args);
    }
    let status = cmd.status().with_context(|_| {
        format!(
            "Could not run `cargo package` for manifest {:?}.",
            manifest_path
        )
    })?;
    if !status.success() {
        bail!("`cargo package` failed to run.");
    }
    let crate_path = target_dir
        .join("package")
        .join(&format!("{}-{}.crate", pkg.name, pkg.version));
    if !crate_path.exists() {
        bail!(
            "Could not find crate after `cargo package` at {:?}",
            crate_path
        );
    }
    Ok(crate_path)
}

/// Compute checksum for a `.crate` file.
pub(crate) fn cksum(path: &Path) -> Result<String, Error> {
    let mut hasher = sha2::Sha256::default();
    let mut file = fs::File::open(&path)
        .with_context(|_| format!("Could not open crate file `{}`.", path.display()))?;
    io::copy(&mut file, &mut hasher).unwrap();
    Ok(hex::encode(hasher.finalize()))
}

pub(crate) fn extract_crate(crate_path: &Path) -> Result<(tempfile::TempDir, PathBuf), Error> {
    let crate_file = fs::File::open(crate_path)
        .with_context(|_| format!("Failed to open `{}`.", crate_path.display()))?;
    let tmp_dir = tempfile::tempdir().unwrap();
    let gz = flate2::read::GzDecoder::new(crate_file);
    let mut tar = tar::Archive::new(gz);
    let prefix = crate_path.file_stem().unwrap();
    for entry in tar.entries()? {
        let mut entry = entry.with_context(|_| "Failed to iterate over archive.")?;
        let entry_path = entry
            .path()
            .with_context(|_| "Failed to read entry path.")?
            .into_owned();
        if !entry_path.starts_with(prefix) {
            bail!(
                "Expected .crate file to contain entries rooted in `{}` directory, found `{}`.",
                prefix.to_str().unwrap(),
                entry_path.display()
            );
        }
        entry
            .unpack_in(tmp_dir.path())
            .with_context(|_| format!("Failed to unpack entry at `{}`.", entry_path.display()))?;
    }
    let pkg_path = tmp_dir.path().join(prefix);
    Ok((tmp_dir, pkg_path))
}

/// Repo-relative path to a package.
pub(crate) fn pkg_path(name: &str) -> PathBuf {
    let name = name.to_lowercase();
    match name.len() {
        1 => Path::new("1").join(&name),
        2 => Path::new("2").join(&name),
        3 => Path::new("3").join(&name[..1]).join(&name),
        _ => Path::new(&name[0..2]).join(&name[2..4]).join(&name),
    }
}

pub(crate) fn vers_eq(v1: &Version, v2: &Version) -> bool {
    // Unfortunately semver ignores build.
    v1 == v2 && v1.build == v2.build
}

pub(crate) fn crate_walker(index: &Path) -> impl Iterator<Item = walkdir::Result<DirEntry>> {
    WalkDir::new(index)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name();
            name != "config.json" && name != ".git" && name != ".cargo-index-lock"
        })
        .filter(|e| match e {
            Ok(e) => e.file_type().is_file(),
            _ => true,
        })
}
