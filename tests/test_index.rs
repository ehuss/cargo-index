mod support;
use self::support::{
    cargo_index, init_index, matches, package, validate, CargoConfig, IndexBuilder,
};
use reg_index::IndexPackage;
use std::fs;
use std::path::Path;

#[test]
fn test_init() {
    let index = IndexBuilder::new().api(false).build();
    assert_eq!(
        fs::read_to_string(index.index_path.join("config.json")).unwrap(),
        format!("{{\n  \"dl\": \"{}\"\n}}", index.dl_pattern_url)
    );
    validate(&index, false);
}

#[test]
fn test_init_api() {
    let index = init_index();
    assert_eq!(
        fs::read_to_string(index.index_path.join("config.json")).unwrap(),
        format!(
            "{{\n  \"dl\": \"{}\",\n  \"api\": \"{}\"\n}}",
            index.dl_pattern_url, index.api_url
        )
    );
    validate(&index, false);
}

#[test]
fn test_init_bad_path() {
    let tmp_dir = tempfile::tempdir().unwrap();
    cargo_index("init")
        .index(tmp_dir.path())
        .arg("--dl=https://example.com")
        .with_status(1)
        .run();
}

#[test]
fn test_metadata() {
    let foo_pkg = package("foo", "0.1.0").build();
    let (stdout, _stderr) = cargo_index("metadata")
        .index_url("https://example.com")
        .manifest(foo_pkg.join("Cargo.toml"))
        .run();
    let reg_pkg: IndexPackage = serde_json::from_str(&stdout).unwrap();
    // Assume the checksum is correct.
    let expected = format!(
        "{{\"name\":\"foo\",\"vers\":\"0.1.0\",\
         \"deps\":[],\"features\":{{}},\"cksum\":\"{}\",\"yanked\":false,\"links\":null}}\n",
        reg_pkg.cksum
    );
    assert_eq!(stdout, expected);

    // Try with a relative path.
    let foo_path = foo_pkg.path();
    let parent = foo_path.parent().unwrap();
    let relative = Path::new(foo_path.file_name().unwrap()).join("Cargo.toml");
    let (stdout, _stderr) = cargo_index("metadata")
        .index_url("https://example.com")
        .cwd(parent)
        .manifest(relative)
        .run();
    assert_eq!(stdout, expected);
}

#[test]
fn test_metadata_crate() {
    let foo_pkg = package("foo", "0.1.0").build();
    foo_pkg.cargo_package();
    let (stdout, _stderr) = cargo_index("metadata")
        .index_url("https://example.com")
        .arg("--crate")
        .arg(foo_pkg.join("target/package/foo-0.1.0.crate"))
        .run();
    let reg_pkg: IndexPackage = serde_json::from_str(&stdout).unwrap();
    // Assume the checksum is correct.
    let expected = format!(
        "{{\"name\":\"foo\",\"vers\":\"0.1.0\",\
         \"deps\":[],\"features\":{{}},\"cksum\":\"{}\",\"yanked\":false,\"links\":null}}\n",
        reg_pkg.cksum
    );
    assert_eq!(stdout, expected);
}

#[test]
fn test_add() {
    let index = init_index();
    let foo_pkg = package("foo", "0.1.0").build();
    foo_pkg.index_add(&index);
    matches(&fs::read_to_string(index.index_path.join("3/f/foo")).unwrap(),
        "{\"name\":\"foo\",\"vers\":\"0.1.0\",\"deps\":[],\"features\":{},\"cksum\":\"<CKSUM>\",\"yanked\":false,\"links\":null}\n");
    validate(&index, true);
}

#[test]
fn test_add_upload() {
    let index = init_index();
    let foo_pkg = package("foo", "0.1.0").build();
    foo_pkg.index_add(&index);
    assert!(index.dl_path.join("foo/foo-0.1.0.crate").exists());
    validate(&index, true)
}

#[test]
fn test_add_crate() {
    let index = init_index();
    let foo_pkg = package("foo", "0.1.0").build();
    foo_pkg.cargo_package();
    let krate = foo_pkg.join("target/package/foo-0.1.0.crate");
    cargo_index("add")
        .index(&index.index_path)
        .index_url("https://example.com")
        .arg("--crate")
        .arg(krate)
        .arg("--upload")
        .arg(&index.dl_pattern_path)
        .run();
    matches(&fs::read_to_string(index.index_path.join("3/f/foo")).unwrap(),
        "{\"name\":\"foo\",\"vers\":\"0.1.0\",\"deps\":[],\"features\":{},\"cksum\":\"<CKSUM>\",\"yanked\":false,\"links\":null}\n");
    validate(&index, true);
}

#[test]
fn test_add_errors() {
    let index = init_index();
    let foo_pkg = package("foo", "0.1.0").build();
    foo_pkg.index_add(&index);
    cargo_index("add")
        .manifest(foo_pkg.join("Cargo.toml"))
        .index(&index.index_path)
        .index_url("https://example.com")
        .with_status(1)
        .with_stderr_contains("Error: Package `foo` version `0.1.0` is already in the index.")
        .run();
}

#[test]
fn test_add_force() {
    // TODO: Finish this.
    let index = init_index();
    let foo_pkg = package("foo", "0.1.0").build();
    foo_pkg.index_add(&index);
    let expected_index = "{\"name\":\"foo\",\"vers\":\"0.1.0\",\"deps\":[],\"features\":{},\"cksum\":\"<CKSUM>\",\"yanked\":false,\"links\":null}\n";
    matches(
        &fs::read_to_string(index.index_path.join("3/f/foo")).unwrap(),
        expected_index,
    );
    cargo_index("add")
        .manifest(foo_pkg.join("Cargo.toml"))
        .index(&index.index_path)
        .index_url("https://example.com")
        .arg("--force")
        .with_status(0)
        .run();
    validate(&index, true);
    // Nothing should have changed when the same package is added.
    matches(
        &fs::read_to_string(index.index_path.join("3/f/foo")).unwrap(),
        expected_index,
    );
}

#[test]
fn test_add_renamed() {
    let index = init_index();
    CargoConfig::new().alt(&index).build();

    index.add_package("bar", "0.1.0");
    let foo_pkg = package("foo", "0.1.0")
        .file(
            "Cargo.toml",
            r#"
            [package]
            name = "foo"
            version = "0.1.0"
            [dependencies]
            baralt = { version = "0.1", package = "bar", registry = "myalt" }
        "#,
        )
        .build();
    foo_pkg.index_add(&index);
    matches(
        &fs::read_to_string(index.index_path.join("3/f/foo")).unwrap(),
        "{\"name\":\"foo\",\"vers\":\"0.1.0\",\"deps\":[\
         {\"name\":\"baralt\",\"req\":\"^0.1\",\"features\":[],\"optional\":false,\
         \"default_features\":true,\"target\":null,\"kind\":\"normal\",\
         \"registry\":null,\"package\":\"bar\"}],\
         \"features\":{},\"cksum\":\"<CKSUM>\",\"yanked\":false,\"links\":null}\n",
    );
    validate(&index, true);
}

#[test]
fn test_add_alt_registry() {
    let index = init_index();
    let alt_index = IndexBuilder::new().name("alt").build();
    CargoConfig::new().alt(&alt_index).build();
    let _bar_pkg = alt_index.add_package("bar", "0.1.0");

    let foo_pkg = package("foo", "0.1.0")
        .file(
            "Cargo.toml",
            r#"
            [package]
            name = "foo"
            version = "0.1.0"
            [dependencies]
            bar = { version = "0.1", registry = "myalt" }
        "#,
        )
        .build();
    foo_pkg.index_add(&index);
    matches(
        &fs::read_to_string(index.index_path.join("3/f/foo")).unwrap(),
        &format!(
            "{{\"name\":\"foo\",\"vers\":\"0.1.0\",\"deps\":[\
             {{\"name\":\"bar\",\"req\":\"^0.1\",\"features\":[],\"optional\":false,\
             \"default_features\":true,\"target\":null,\"kind\":\"normal\",\
             \"registry\":\"{}\",\"package\":null}}],\
             \"features\":{{}},\"cksum\":\"<CKSUM>\",\"yanked\":false,\"links\":null}}\n",
            alt_index.index_url
        ),
    );
    validate(&index, true);
}

#[test]
fn test_add_crates_io() {
    let alt_index = IndexBuilder::new().name("alt").build();
    CargoConfig::new().alt(&alt_index).build();
    let foo_pkg = package("foo", "0.1.0")
        .file(
            "Cargo.toml",
            r#"
            [package]
            name = "foo"
            version = "0.1.0"
            [dependencies]
            bitflags = "1.0"
        "#,
        )
        .file("src/lib.rs", "extern crate bitflags;")
        .build();
    foo_pkg.index_add(&alt_index);
    matches(
        &fs::read_to_string(alt_index.index_path.join("3/f/foo")).unwrap(),
        "{\"name\":\"foo\",\"vers\":\"0.1.0\",\"deps\":[\
         {\"name\":\"bitflags\",\"req\":\"^1.0\",\"features\":[],\"optional\":false,\
         \"default_features\":true,\"target\":null,\"kind\":\"normal\",\
         \"registry\":\"<CRATES.IO>\",\"package\":null}],\
         \"features\":{},\"cksum\":\"<CKSUM>\",\"yanked\":false,\"links\":null}\n",
    );
    validate(&alt_index, true);

    let bar_pkg = package("bar", "0.1.0")
        .file(
            "Cargo.toml",
            r#"
            [package]
            name = "bar"
            version = "0.1.0"
            [dependencies]
            foo = { version = "0.1", registry = "myalt" }
        "#,
        )
        .file("src/lib.rs", "extern crate foo;")
        .build();
    bar_pkg.index_add(&alt_index);
    matches(
        &fs::read_to_string(alt_index.index_path.join("3/b/bar")).unwrap(),
        "{\"name\":\"bar\",\"vers\":\"0.1.0\",\"deps\":[\
         {\"name\":\"foo\",\"req\":\"^0.1\",\"features\":[],\"optional\":false,\
         \"default_features\":true,\"target\":null,\"kind\":\"normal\",\
         \"registry\":null,\"package\":null}],\
         \"features\":{},\"cksum\":\"<CKSUM>\",\"yanked\":false,\"links\":null}\n",
    );
    validate(&alt_index, true);
}

#[test]
fn test_add_links() {
    let index = init_index();
    let foo_pkg = package("foo", "0.1.0")
        .file(
            "Cargo.toml",
            r#"
            [package]
            name = "foo"
            version = "0.1.0"
            links = "somepkg"
        "#,
        )
        .file("build.rs", "fn main() {}")
        .build();
    foo_pkg.index_add(&index);
    matches(
        &fs::read_to_string(index.index_path.join("3/f/foo")).unwrap(),
        "{\"name\":\"foo\",\"vers\":\"0.1.0\",\"deps\":[],\
         \"features\":{},\"cksum\":\"<CKSUM>\",\"yanked\":false,\"links\":\"somepkg\"}\n",
    );
    validate(&index, true);
}

#[test]
fn test_target_cfg() {
    let index = init_index();
    CargoConfig::new().alt(&index).build();

    index.add_package("bar", "0.1.0");
    let foo_pkg = package("foo", "0.1.0")
        .file(
            "Cargo.toml",
            r#"
            [package]
            name = "foo"
            version = "0.1.0"
            [target.'cfg(windows)'.dependencies]
            bar = { version = "0.1", registry = "myalt" }
        "#,
        )
        .build();
    foo_pkg.index_add(&index);
    matches(
        &fs::read_to_string(index.index_path.join("3/f/foo")).unwrap(),
        "{\"name\":\"foo\",\"vers\":\"0.1.0\",\"deps\":[\
         {\"name\":\"bar\",\"req\":\"^0.1\",\"features\":[],\"optional\":false,\
         \"default_features\":true,\"target\":\"cfg(windows)\",\"kind\":\"normal\",\
         \"registry\":null,\"package\":null}],\
         \"features\":{},\"cksum\":\"<CKSUM>\",\"yanked\":false,\"links\":null}\n",
    );
    validate(&index, true);
}

#[test]
fn test_yank() {
    let index = init_index();
    index.add_package("foo", "0.1.0");
    index.add_package("foo", "0.1.1");
    index.add_package("foo", "0.1.2");
    cargo_index("yank")
        .index(&index.index_path)
        .arg("-p=foo")
        .arg("--version=0.1.0")
        .run();
    matches(&fs::read_to_string(index.index_path.join("3/f/foo")).unwrap(),
        "{\"name\":\"foo\",\"vers\":\"0.1.0\",\"deps\":[],\"features\":{},\"cksum\":\"<CKSUM>\",\"yanked\":true,\"links\":null}\n\
         {\"name\":\"foo\",\"vers\":\"0.1.1\",\"deps\":[],\"features\":{},\"cksum\":\"<CKSUM>\",\"yanked\":false,\"links\":null}\n\
         {\"name\":\"foo\",\"vers\":\"0.1.2\",\"deps\":[],\"features\":{},\"cksum\":\"<CKSUM>\",\"yanked\":false,\"links\":null}\n");
    cargo_index("unyank")
        .index(&index.index_path)
        .arg("-p=foo")
        .arg("--version=0.1.0")
        .run();
    matches(&fs::read_to_string(index.index_path.join("3/f/foo")).unwrap(),
        "{\"name\":\"foo\",\"vers\":\"0.1.0\",\"deps\":[],\"features\":{},\"cksum\":\"<CKSUM>\",\"yanked\":false,\"links\":null}\n\
         {\"name\":\"foo\",\"vers\":\"0.1.1\",\"deps\":[],\"features\":{},\"cksum\":\"<CKSUM>\",\"yanked\":false,\"links\":null}\n\
         {\"name\":\"foo\",\"vers\":\"0.1.2\",\"deps\":[],\"features\":{},\"cksum\":\"<CKSUM>\",\"yanked\":false,\"links\":null}\n");
}

#[test]
fn test_yank_errors() {
    let index = init_index();
    index.add_package("foo", "0.1.0");
    cargo_index("yank")
        .index(&index.index_path)
        .arg("-p=bar")
        .arg("--version=0.1.0")
        .with_status(1)
        .with_stderr("Error: Package `bar` is not in the index.")
        .run();
    cargo_index("yank")
        .index(&index.index_path)
        .arg("-p=foo")
        .arg("--version=0.1.1")
        .with_status(1)
        .with_stderr("Error: Version `0.1.1` for package `foo` not found.")
        .run();

    cargo_index("unyank")
        .index(&index.index_path)
        .arg("-p=foo")
        .arg("--version=0.1.0")
        .with_status(1)
        .with_stderr("Error: `foo:0.1.0` is not yanked!")
        .run();
    cargo_index("yank")
        .index(&index.index_path)
        .arg("-p=foo")
        .arg("--version=0.1.0")
        .run();
    cargo_index("yank")
        .index(&index.index_path)
        .arg("-p=foo")
        .arg("--version=0.1.0")
        .with_status(1)
        .with_stderr("Error: `foo:0.1.0` is already yanked!")
        .run();
}

#[test]
fn test_list() {
    let index = init_index();
    index.add_package("foo", "0.1.0");
    index.add_package("foo", "0.1.1");
    index.add_package("foo", "1.0.0");
    let (stdout, _stderr) = cargo_index("list")
        .index(&index.index_path)
        .arg("-p=foo")
        .run();
    matches(&stdout,
        "{\"name\":\"foo\",\"vers\":\"0.1.0\",\"deps\":[],\"features\":{},\"cksum\":\"<CKSUM>\",\"yanked\":false,\"links\":null}\n\
         {\"name\":\"foo\",\"vers\":\"0.1.1\",\"deps\":[],\"features\":{},\"cksum\":\"<CKSUM>\",\"yanked\":false,\"links\":null}\n\
         {\"name\":\"foo\",\"vers\":\"1.0.0\",\"deps\":[],\"features\":{},\"cksum\":\"<CKSUM>\",\"yanked\":false,\"links\":null}\n");

    let (stdout, _stderr) = cargo_index("list")
        .index(&index.index_path)
        .arg("-p=foo")
        .arg("--version=0.1.1")
        .run();
    matches(&stdout,
        "{\"name\":\"foo\",\"vers\":\"0.1.1\",\"deps\":[],\"features\":{},\"cksum\":\"<CKSUM>\",\"yanked\":false,\"links\":null}\n");

    let (stdout, _stderr) = cargo_index("list")
        .index(&index.index_path)
        .arg("-p=foo")
        .arg("--version=<1")
        .run();
    matches(&stdout,
        "{\"name\":\"foo\",\"vers\":\"0.1.0\",\"deps\":[],\"features\":{},\"cksum\":\"<CKSUM>\",\"yanked\":false,\"links\":null}\n\
         {\"name\":\"foo\",\"vers\":\"0.1.1\",\"deps\":[],\"features\":{},\"cksum\":\"<CKSUM>\",\"yanked\":false,\"links\":null}\n");
}

#[test]
fn test_list_errors() {
    let index = init_index();
    index.add_package("foo", "0.1.0");
    cargo_index("list")
        .index(&index.index_path)
        .arg("-p=bar")
        .with_status(1)
        .with_stderr("Error: Package `bar` is not in the index.")
        .run();
    cargo_index("list")
        .index(&index.index_path)
        .arg("-p=foo")
        .arg("--version=1.0")
        .with_status(1)
        .with_stderr("Error: No entries found for `foo` that match version `1.0`.")
        .run();
    cargo_index("list")
        .index(&index.index_path)
        .arg("-p=foo")
        .arg("--version=foo")
        .with_status(1)
        .with_stderr("Error: unexpected character 'f' while parsing major version number")
        .run();
}

#[test]
fn test_add_inferred_ws_path() {
    let index = init_index();
    let foo_pkg = package("foo", "0.1.0")
        .file(
            "Cargo.toml",
            r#"
            [package]
            name = "foo"
            version = "0.1.0"
            [workspace]
            members = ["bar"]
        "#,
        )
        .file(
            "bar/Cargo.toml",
            r#"
            [package]
            name = "bar"
            version = "0.1.0"
        "#,
        )
        .file("bar/src/lib.rs", "")
        .build();
    let bar_path = foo_pkg.join("bar");
    cargo_index("add")
        .cwd(&bar_path)
        .index(&index.index_path)
        .index_url("https://example.com")
        .arg("--upload")
        .arg(&index.dl_pattern_path)
        .run();
    matches(&fs::read_to_string(index.index_path.join("3/b/bar")).unwrap(),
        "{\"name\":\"bar\",\"vers\":\"0.1.0\",\"deps\":[],\"features\":{},\"cksum\":\"<CKSUM>\",\"yanked\":false,\"links\":null}\n");
    validate(&index, true);
}

#[test]
fn test_package_args() {
    let foo_pkg = package("foo", "0.1.0").file("src/lib.rs", "asdf").build();
    cargo_index("metadata")
        .cwd(&foo_pkg.path())
        .index_url("https://example.com")
        .with_stderr_contains("asdf")
        .with_status(1)
        .run();
    let (stdout, _stderr) = cargo_index("metadata")
        .cwd(&foo_pkg.path())
        .index_url("https://example.com")
        .arg("--")
        .arg("--no-verify")
        .run();
    matches(
        &stdout,
        "{\"name\":\"foo\",\"vers\":\"0.1.0\",\
         \"deps\":[],\"features\":{},\"cksum\":\"<CKSUM>\",\"yanked\":false,\"links\":null}\n",
    );
}
