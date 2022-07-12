use crate::util::signature;
use anyhow::{bail, Context, Error};
use std::{fs, path::Path};

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
        .with_context(|| format!("git failed to initialize `{}`", path.display()))?;
    let config_json = match api {
        Some(api) => format!(
            "{{\n  \"dl\": \"{}\",\n  \"api\": \"{}\"\n}}",
            dl,
            api.trim_end_matches('/')
        ),
        None => format!("{{\n  \"dl\": \"{}\"\n}}", dl),
    };
    let json_path = path.join("config.json");
    fs::write(&json_path, config_json).with_context(|| "Failed to write config.json")?;

    let mut index = repo.index()?;
    index.add_path(Path::new("config.json"))?;
    index.write()?;
    let id = index.write_tree()?;
    let tree = repo.find_tree(id)?;
    let sig = signature(&repo)?;
    repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])?;
    Ok(())
}
