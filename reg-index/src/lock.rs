use failure::Error;
use fs2::FileExt;
use std::{fs::File, path::Path};

pub struct Lock {
    #[allow(unused)]
    file: File,
}

impl Lock {
    pub fn new_exclusive(path: impl AsRef<Path>) -> Result<Lock, Error> {
        let file = File::open(path.as_ref())?;
        file.lock_exclusive()?;
        Ok(Lock { file })
    }

    pub fn new_shared(path: impl AsRef<Path>) -> Result<Lock, Error> {
        let file = File::open(path.as_ref())?;
        file.lock_shared()?;
        Ok(Lock { file })
    }
}
