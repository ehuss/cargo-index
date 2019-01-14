#![warn(missing_docs)]

/*!
This library is for accessing and manipulating a Cargo registry index.

A very basic example:

```rust
# fn main() -> Result<(), failure::Error> {
# std::env::set_var("GIT_AUTHOR_NAME", "Index Admin");
# std::env::set_var("GIT_AUTHOR_EMAIL", "admin@example.com");
# let tmp_dir = tempfile::tempdir().unwrap();
# let index_path = tmp_dir.path().join("index");
# let index_url = "https://example.com/";
# let project = tmp_dir.path().join("foo");
# let status = std::process::Command::new("cargo")
#     .args(&["new", "--vcs=none", project.to_str().unwrap()])
#     .status()?;
# assert!(status.success());
# let manifest_path = project.join("Cargo.toml");
// Initialize a new index.
reg_index::init(&index_path, "https://example.com", None)?;
// Add a package to the index.
reg_index::add(&index_path, index_url, Some(&manifest_path), None, None)?;
// Packages can be yanked.
reg_index::yank(&index_path, "foo", "0.1.0")?;
// Get the metadata for the new entry.
let pkgs = reg_index::list(&index_path, "foo", None)?;
// Displays something like:
// {"name":"foo","vers":"0.1.0","deps":[],"features":{},"cksum":"d87f097fcc13ae97736a7d8086fb70a0499f3512f0fe1fe82e6422f25f567c83","yanked":true,"links":null}
println!("{}", serde_json::to_string(&pkgs[0])?);
# Ok(())
# }
```

See TODO for documentation about Cargo registries.

## Locking
The functions here perform simple filesystem locking to ensure multiple
commands running at the same time do not interfere with one another. This
requires that the filesystem supports locking.
*/

use failure::{bail, format_err, Error, ResultExt};
use git2;
use semver::{Version, VersionReq};
use serde_derive::{Deserialize, Serialize};
use sha2::Digest;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
};
use url::Url;
use walkdir::{DirEntry, WalkDir};

mod lock;
use self::lock::Lock;

/// An entry for a single version of a package in the index.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexPackage {
    /// The name of the package.
    pub name: String,
    /// The version of the package.
    pub vers: Version,
    /// List of direct dependencies of the package.
    pub deps: Vec<IndexDependency>,
    /// Cargo features defined in the package.
    pub features: BTreeMap<String, Vec<String>>,
    /// Checksum of the `.crate` file.
    pub cksum: String,
    /// Whether or not this package is yanked.
    pub yanked: bool,
    /// Optional string that is the name of a native library the package is
    /// linking to.
    pub links: Option<String>,
    #[doc(hidden)]
    #[serde(skip)]
    __nonexhaustive: (),
}

/// A dependency of a package.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexDependency {
    /// Name of the dependency.
    ///
    /// If the dependency is renamed from the original package name,
    /// this is the new name. The original package name is stored in
    /// the `package` field.
    pub name: String,
    /// The semver requirement for this dependency.
    pub req: VersionReq,
    /// List of features enabled for this dependency.
    pub features: Vec<String>,
    /// Whether or not this is an optional dependency.
    pub optional: bool,
    /// Whether or not default features are enabled.
    pub default_features: bool,
    /// The target platform for the dependency.
    pub target: Option<String>,
    /// The dependency kind.
    // Required, but crates.io has some broken missing entries.
    #[serde(default, deserialize_with = "parse_dependency_kind")]
    pub kind: cargo_metadata::DependencyKind,
    /// The URL of the index of the registry where this dependency is from.
    ///
    /// If not specified or null, it is assumed the dependency is in the
    /// current registry.
    #[serde(default, with = "url_serde")]
    pub registry: Option<Url>,
    /// If the dependency is renamed, this is a string of the actual package
    /// name. If None, this dependency is not renamed.
    pub package: Option<String>,
    #[doc(hidden)]
    #[serde(skip)]
    __nonexhaustive: (),
}

fn parse_dependency_kind<'de, D>(d: D) -> Result<cargo_metadata::DependencyKind, D::Error>
where
    D: serde::Deserializer<'de>,
{
    serde::Deserialize::deserialize(d).map(|x: Option<_>| x.unwrap_or_default())
}

/// The configuration file of the index.
///
/// This is stored in the root of the index repo as `config.json`.
#[derive(Serialize, Deserialize)]
pub struct IndexConfig {
    /// URL that Cargo uses to download crates.
    ///
    /// This can have the markers `{crate}` and `{version}`. If the markers
    /// are not present, Cargo automatically appends
    /// `/{crate}/{version}/download` to the end.
    #[serde(with = "url_serde")]
    pub dl: Url,
    /// URL that Cargo uses for the web API (publish/yank/search/etc.).
    ///
    /// This is optional. If not specified, Cargo will refuse to publish to
    /// this registry.
    #[serde(default, with = "url_serde")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<Url>,
    #[doc(hidden)]
    #[serde(skip)]
    __nonexhaustive: (),
}

struct MetaInfo {
    index_pkg: IndexPackage,
    crate_path: PathBuf,
}

/// Initialize a new registry index.
///
/// See [`IndexConfig`] for a description of the `dl` and `api` parameters.
///
/// [`IndexConfig`]: struct.IndexConfig.html
pub fn init(path: impl AsRef<Path>, dl: &str, api: Option<&str>) -> Result<(), Error> {
    let path = path.as_ref();
    if path.exists() {
        bail!(
            "Path `{}` already exists. This command requires a non-existent path to create.",
            path.display()
        );
    }
    let repo = git2::Repository::init(path)
        .with_context(|_| format!("git failed to initialize `{}`", path.display()))?;
    let config_json = match api {
        Some(api) => format!(
            "{{\n  \"dl\": \"{}\",\n  \"api\": \"{}\"\n}}",
            dl,
            api.trim_end_matches('/')
        ),
        None => format!("{{\n  \"dl\": \"{}\"\n}}", dl),
    };
    let json_path = path.join("config.json");
    fs::write(&json_path, config_json).with_context(|_| "Failed to write config.json")?;

    let mut index = repo.index()?;
    index.add_path(Path::new("config.json"))?;
    index.write()?;
    let id = index.write_tree()?;
    let tree = repo.find_tree(id)?;
    let sig = signature(&repo)?;
    repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])?;
    Ok(())
}

fn signature(repo: &git2::Repository) -> Result<git2::Signature, Error> {
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

fn metadata_reg(
    index_url: &str,
    manifest_path: Option<&Path>,
    crate_path: Option<&Path>,
    package_args: Option<&Vec<String>>,
) -> Result<MetaInfo, Error> {
    let mut cmd = cargo_metadata::MetadataCommand::new();
    if let Some(path) = manifest_path {
        cmd.manifest_path(path);
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
        .find(|p| Path::new(&p.manifest_path) == actual_manifest_path)
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
            &metadata.target_directory,
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
        cksum,
        yanked: false,
        links: pkg.links.clone(),
        __nonexhaustive: (),
    };
    let info = MetaInfo {
        index_pkg,
        crate_path,
    };
    Ok(info)
}

/// Call `cargo package` to generate a `.crate` file.
fn cargo_package(
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
fn cksum(path: &Path) -> Result<String, Error> {
    let mut hasher = sha2::Sha256::default();
    let mut file = fs::File::open(&path)
        .with_context(|_| format!("Could not open crate file `{}`.", path.display()))?;
    io::copy(&mut file, &mut hasher).unwrap();
    Ok(hex::encode(hasher.result()))
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

fn extract_crate(crate_path: &Path) -> Result<(tempfile::TempDir, PathBuf), Error> {
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

fn add_reg(
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
fn git_add(repo: &git2::Repository, path: &Path, msg: &str) -> Result<(), Error> {
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

/// Repo-relative path to a package.
fn pkg_path(name: &str) -> PathBuf {
    let name = name.to_lowercase();
    match name.len() {
        1 => Path::new("1").join(&name),
        2 => Path::new("2").join(&name),
        3 => Path::new("3").join(&name[..1]).join(&name),
        _ => Path::new(&name[0..2]).join(&name[2..4]).join(&name),
    }
}

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
                Ok((line.to_string(), 0))
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
    mut cb: impl FnMut(&IndexPackage),
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
            cb(&pkg);
        }
    } else {
        for entry in crate_walker(index) {
            let entry = entry?;
            for pkg in _list(
                index,
                entry.file_name().to_str().unwrap(),
                version_req.as_ref(),
            )? {
                cb(&pkg);
            }
        }
    };
    drop(lock);
    Ok(())
}

fn _list(
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
    let mut found_err = _validate(&mut crate_map, &index, crates)?;
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
            .with_context(|_| format!("Failed to read `{}`.", path.display())));
        let mut seen = HashSet::new();
        for line in contents.lines() {
            let pkg: IndexPackage = t!(serde_json::from_str(line).with_context(|_| format!(
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

/// Return the configuration file in an index.
pub fn load_config(index: impl AsRef<Path>) -> Result<IndexConfig, Error> {
    let path = index.as_ref().join("config.json");
    let f =
        fs::File::open(&path).with_context(|_| format!("Failed to open `{}`.", path.display()))?;
    let index_cfg: IndexConfig = serde_json::from_reader(f)
        .with_context(|_| format!("Failed to deserialize `{}`.", path.display()))?;
    Ok(index_cfg)
}

fn vers_eq(v1: &Version, v2: &Version) -> bool {
    // Unfortunately semver ignores build.
    v1 == v2 && v1.build == v2.build
}

fn crate_walker(index: &Path) -> impl Iterator<Item = walkdir::Result<DirEntry>> {
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
