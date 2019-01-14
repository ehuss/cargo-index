use super::{root, PathExt, TestIndex};
use std::fs;

pub struct CargoConfig {
    result: Vec<String>,
}

impl CargoConfig {
    pub fn new() -> CargoConfig {
        CargoConfig { result: Vec::new() }
    }

    pub fn alt(mut self, index: &TestIndex) -> CargoConfig {
        self.result.push(format!(
            r#"
            [registries.myalt]
            index = '{}'
        "#,
            index.index_url
        ));
        self
    }

    pub fn build(self) {
        let dot_cargo = root().join(".cargo");
        assert!(!dot_cargo.exists());
        dot_cargo.mkdir_p();
        fs::write(&dot_cargo.join("config"), self.result.join("")).unwrap();
    }
}
