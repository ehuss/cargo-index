use std::{fs, path::Path};

pub trait PathExt {
    fn rm_rf(&self);
    fn mkdir_p(&self);
}

impl PathExt for Path {
    fn rm_rf(&self) {
        if !self.exists() {
            return;
        }
        fs::remove_dir_all(self).unwrap_or_else(|e| panic!("failed to rm_rf {:?}: {}", self, e));
    }

    fn mkdir_p(&self) {
        fs::create_dir_all(self).unwrap_or_else(|e| panic!("failed to mkdir_p {:?}: {}", self, e))
    }
}
