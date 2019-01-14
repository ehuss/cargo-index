use super::{cargo_index, cargo_package, root, PathExt, TestIndex};
use std::{
    collections::hash_map::HashMap,
    fs,
    path::{Path, PathBuf},
};

pub struct PackageBuilder {
    name: String,
    version: String,
    files: HashMap<PathBuf, String>,
}

pub struct Package {
    path: PathBuf,
}

impl PackageBuilder {
    pub fn new(name: &str, version: &str) -> PackageBuilder {
        PackageBuilder {
            name: name.to_string(),
            version: version.to_string(),
            files: HashMap::new(),
        }
    }

    pub fn file(mut self, path: impl AsRef<Path>, body: &str) -> Self {
        self._file(path.as_ref(), body);
        self
    }

    fn _file(&mut self, path: impl AsRef<Path>, body: &str) {
        let path = path.as_ref().to_path_buf();
        if self.files.contains_key(&path) {
            panic!("{:?} is already set", path);
        } else {
            self.files.insert(path, body.to_string());
        }
    }

    pub fn build(mut self) -> Package {
        let dirname = format!("{}-{}", self.name, self.version);
        let pkg_root = root().join(dirname);
        pkg_root.mkdir_p();
        if !self.files.contains_key(&PathBuf::from("src/lib.rs"))
            && !self.files.contains_key(&PathBuf::from("src/main.rs"))
        {
            self._file("src/lib.rs", "");
        }
        if !self.files.contains_key(&PathBuf::from("Cargo.toml")) {
            self._file(
                "Cargo.toml",
                &format!(
                    r#"
[package]
name = "{}"
version = "{}"
"#,
                    self.name, self.version
                ),
            );
        }
        for (path, body) in self.files {
            let abs = pkg_root.join(path);
            abs.parent().unwrap().mkdir_p();
            fs::write(&abs, body).unwrap_or_else(|e| panic!("Failed to write {:?}: {}", abs, e));
        }
        Package { path: pkg_root }
    }
}

impl Package {
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn join(&self, path: impl AsRef<Path>) -> PathBuf {
        self.path.join(path.as_ref())
    }

    pub fn cargo_package(&self) {
        cargo_package(&self.path);
    }

    pub fn index_add(&self, index: &TestIndex) {
        cargo_index("add")
            .manifest(self.join("Cargo.toml"))
            .index(&index.index_path)
            .index_url(&index.index_url)
            .arg("--upload")
            .arg(&index.dl_pattern_path)
            .run();
    }
}
