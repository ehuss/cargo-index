use failure::Error;
use fs2::FileExt;
use std::{
    fs::{File, OpenOptions},
    path::Path,
};

pub struct Lock {
    #[allow(unused)]
    file: File,
}

impl Lock {
    pub fn new_exclusive(path: impl AsRef<Path>) -> Result<Lock, Error> {
        let file = OpenOptions::new()
            .read(true)
            .create(true)
            .write(true)
            .open(path.as_ref().join(".cargo-index-lock"))?;
        file.lock_exclusive()?;
        Ok(Lock { file })
    }

    pub fn new_shared(path: impl AsRef<Path>) -> Result<Lock, Error> {
        let file = OpenOptions::new()
            .read(true)
            .create(true)
            .write(true)
            .open(path.as_ref().join(".cargo-index-lock"))?;
        file.lock_shared()?;
        Ok(Lock { file })
    }
}
