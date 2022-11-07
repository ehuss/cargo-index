#![macro_use]
//! Utilities for tests.

use std::{
    cell::Cell,
    env,
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::Command,
    str,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Once,
    },
};
use url::Url;

macro_rules! t {
    ($e:expr) => {
        match $e {
            Ok(e) => e,
            Err(e) => panic!("{} failed with {}", stringify!($e), e),
        }
    };
}

mod config;
mod package;
mod paths;

use self::package::{Package, PackageBuilder};
pub use self::{config::CargoConfig, paths::PathExt};

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
thread_local!(static TASK_ID: usize = NEXT_ID.fetch_add(1, Ordering::SeqCst));

fn init() {
    static GLOBAL_INIT: Once = Once::new();
    thread_local!(static LOCAL_INIT: Cell<bool> = Cell::new(false));
    GLOBAL_INIT.call_once(|| {
        global_root().mkdir_p();
        // Appveyor runs without git user/email configured.
        env::set_var("GIT_AUTHOR_NAME", "Index Admin");
        env::set_var("GIT_AUTHOR_EMAIL", "admin@example.com");
    });
    LOCAL_INIT.with(|i| {
        if i.get() {
            return;
        }
        i.set(true);
        let p = root();
        p.rm_rf();
        p.mkdir_p();
    })
}

fn global_root() -> PathBuf {
    let mut path = env::current_exe().unwrap();
    path.pop(); // chop off exe name
    path.pop(); // chop off 'debug'
    if path.file_name().and_then(|s| s.to_str()) != Some("target") {
        path.pop();
    }
    path.join("cargo-index-test")
}

/// The root directory for the current test.
pub fn root() -> PathBuf {
    init();
    global_root().join(&TASK_ID.with(|my_id| format!("t{}", my_id)))
}

/// A builder for constructing and running a `cargo index` command and
/// checking its output.
pub struct TestBuilder {
    ran: bool,
    args: Vec<OsString>,
    cwd: Option<PathBuf>,
    status: i32,
    expected_stderr: Option<String>,
    expected_stderr_contains: Option<String>,
}

impl TestBuilder {
    pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        self.args.push(arg.as_ref().to_os_string());
        self
    }

    pub fn with_status(&mut self, status: i32) -> &mut Self {
        self.status = status;
        self
    }

    pub fn with_stderr(&mut self, output: impl ToString) -> &mut Self {
        self.expected_stderr = Some(output.to_string());
        self
    }

    pub fn with_stderr_contains(&mut self, output: impl ToString) -> &mut Self {
        self.expected_stderr_contains = Some(output.to_string());
        self
    }

    pub fn manifest(&mut self, path: impl AsRef<Path>) -> &mut Self {
        let path = path.as_ref();
        self.arg("--manifest-path");
        self.arg(path);
        self
    }

    pub fn index(&mut self, path: impl AsRef<Path>) -> &mut Self {
        let path = path.as_ref();
        self.arg("--index");
        self.arg(path);
        self
    }

    pub fn index_url(&mut self, url: &str) -> &mut Self {
        self.arg("--index-url");
        self.arg(url);
        self
    }

    pub fn cwd(&mut self, path: impl AsRef<Path>) -> &mut Self {
        self.cwd = Some(path.as_ref().to_path_buf());
        self
    }

    pub fn run(&mut self) -> (String, String) {
        self.ran = true;
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_cargo-index"));
        if let Some(cwd) = &self.cwd {
            cmd.current_dir(cwd);
        }
        let output = cmd
            .args(&self.args)
            .output()
            .expect("Failed to launch cargo-index.");
        let stdout = String::from_utf8(output.stdout).unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        if output.status.code() != Some(self.status) {
            panic!(
                "cargo-index exit status={} expected={}\n--- stderr\n{}\n--- stdout\n{}",
                output.status, self.status, stderr, stdout
            );
        }
        if let Some(expected_stderr) = &self.expected_stderr {
            assert_eq!(expected_stderr, &stderr.trim());
        }
        if let Some(expected_stderr) = &self.expected_stderr_contains {
            assert!(
                stderr.contains(expected_stderr),
                "Could not find, expected:\n{}\nActual stderr:\n{}\n",
                expected_stderr,
                stderr
            );
        }
        (stdout, stderr)
    }
}

impl Drop for TestBuilder {
    fn drop(&mut self) {
        if !self.ran && !std::thread::panicking() {
            panic!("forgot to run this command");
        }
    }
}

pub struct IndexBuilder {
    name: String,
    api: bool,
}

impl IndexBuilder {
    pub fn new() -> IndexBuilder {
        IndexBuilder {
            name: "registry".to_string(),
            api: true,
        }
    }
    pub fn name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }
    pub fn api(mut self, api: bool) -> Self {
        self.api = api;
        self
    }
    pub fn build(self) -> TestIndex {
        TestIndex::new(&self.name, self.api)
    }
}

pub fn init_index() -> TestIndex {
    IndexBuilder::new().build()
}

/// Information after running `cargo index init`.
pub struct TestIndex {
    pub index_path: PathBuf,
    pub index_url: String,
    pub dl_path: PathBuf,
    pub dl_pattern_path: PathBuf,
    pub dl_pattern_url: String,
    pub api_path: PathBuf,
    pub api_url: String,
}

impl TestIndex {
    pub fn new(name: &str, api: bool) -> TestIndex {
        let base = root().join(name);
        let index_path = base.join("index");
        let index_url = Url::from_file_path(&index_path).unwrap().to_string();
        let dl_path = base.join("dl");
        let dl_pattern_path = dl_path.join("{crate}");
        let mut dl_pattern_url = Url::from_file_path(&dl_path).unwrap().to_string();
        dl_pattern_url.push_str("/{crate}/{crate}-{version}.crate");
        let api_path = base.join("api");
        let api_url = Url::from_file_path(&api_path).unwrap().to_string();

        let mut proc = cargo_index("init");
        dl_path.mkdir_p();
        api_path.mkdir_p();
        proc.arg("--index")
            .arg(&index_path)
            .arg(&format!("--dl={}", dl_pattern_url));
        if api {
            proc.arg(&format!("--api={}", api_url));
        }
        proc.run();
        assert!(index_path.exists());
        assert!(index_path.join(".git").exists());

        TestIndex {
            index_path,
            index_url,
            dl_path,
            dl_pattern_path,
            dl_pattern_url,
            api_path,
            api_url,
        }
    }

    pub fn add_package(&self, name: &str, version: &str) -> Package {
        let pkg = package(name, version).build();
        pkg.cargo_package();
        pkg.index_add(self);
        pkg
    }
}

/// Create a `TestBuilder` for running `cargo index`.
pub fn cargo_index(cmd: &str) -> TestBuilder {
    TestBuilder {
        ran: false,
        args: vec![OsString::from("index"), OsString::from(cmd)],
        cwd: None,
        status: 0,
        expected_stderr: None,
        expected_stderr_contains: None,
    }
}

pub fn cargo_package(path: impl AsRef<Path>) {
    let path = path.as_ref();
    let output = Command::new("cargo")
        .args(&["package", "--allow-dirty"])
        .current_dir(path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run `cargo package`: {}", e));
    if output.status.code() != Some(0) {
        let stdout = String::from_utf8(output.stdout).unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        panic!(
            "`cargo package` failed with status {}:\n---stdout---\n{}\n---stderr---\n{}\n",
            output.status, stdout, stderr
        )
    }
}

pub fn package(name: &str, version: &str) -> PackageBuilder {
    PackageBuilder::new(name, version)
}

/// Check if one string matches another.
pub fn matches(actual: &str, expected: &str) {
    let expected = expected.replace(
        "<CRATES.IO>",
        "https://github.com/rust-lang/crates.io-index",
    );
    let expected = regex::escape(&expected);
    let expected = expected.replace("<CKSUM>", "[a-f0-9]{64}");
    let expected = format!("^{}$", expected);
    let re = regex::Regex::new(&expected).unwrap();
    if !re.is_match(actual) {
        panic!("Mismatch actual:\n{}\nexpected:\n{}", actual, expected);
    }
}

/// Validate an index.
pub fn validate(index: &TestIndex, crates: bool) {
    let mut proc = cargo_index("validate");
    proc.arg("--index").arg(&index.index_path);
    if crates {
        proc.arg("--crates").arg(&index.dl_pattern_path);
    }
    proc.run();
}
