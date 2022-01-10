#![warn(missing_docs)]
#![allow(clippy::redundant_closure)]

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
reg_index::add(&index_path, index_url, Some(&manifest_path), None, false, None)?;
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

See https://doc.rust-lang.org/cargo/reference/registries.html for
documentation about Cargo registries.

## Locking
The functions here perform simple filesystem locking to ensure multiple
commands running at the same time do not interfere with one another. This
requires that the filesystem supports locking.
*/

use failure::{Error, ResultExt};
use semver::{Version, VersionReq};
use serde_derive::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};
use url::Url;

mod add;
mod init;
mod list;
mod lock;
mod metadata;
mod util;
mod validate;
mod yank;

pub use add::{add, add_from_crate};
pub use init::init;
pub use list::{list, list_all};
pub use metadata::{metadata, metadata_from_crate};
pub use validate::validate;
pub use yank::{set_yank, unyank, yank};

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
    #[serde(default)]
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
    pub dl: Url,
    /// URL that Cargo uses for the web API (publish/yank/search/etc.).
    ///
    /// This is optional. If not specified, Cargo will refuse to publish to
    /// this registry.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<Url>,
    #[doc(hidden)]
    #[serde(skip)]
    __nonexhaustive: (),
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
