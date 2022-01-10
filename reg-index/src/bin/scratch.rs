use failure::Error;

fn main() -> Result<(), Error> {
    let tmp_dir = tempfile::tempdir().unwrap();
    let index_path = tmp_dir.path().join("index");
    let project = tmp_dir.path().join("foo");
    let status = std::process::Command::new("cargo")
        .args(&["new", project.to_str().unwrap()])
        .status()?;
    assert!(status.success());
    let status = std::process::Command::new("cargo")
        .args(&["package", "--allow-dirty"])
        .current_dir(&project)
        .status()?;
    assert!(status.success());
    let manifest_path = project.join("Cargo.toml");

    // Initialize a new index.
    reg_index::init(&index_path, "https://example.com", None)?;
    // Add a package to the index. This requires a path to a Cargo manifest, and
    // `cargo package` must have been run beforehand.
    reg_index::add(
        &index_path,
        "https://example.com",
        Some(&manifest_path),
        None,
        false,
        None,
    )?;
    // Get the metadata for the new entry.
    let pkgs = reg_index::list(&index_path, "foo", None)?;
    // Displays something like:
    // {"name":"foo","vers":"0.1.0","deps":[],"features":{},"cksum":"d87f097fcc13ae97736a7d8086fb70a0499f3512f0fe1fe82e6422f25f567c83","yanked":false,"links":null}
    println!("{}", serde_json::to_string(&pkgs[0])?);
    Ok(())
}
