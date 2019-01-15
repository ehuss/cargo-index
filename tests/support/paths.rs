use std::{
    fs,
    io::{self, ErrorKind},
    path::Path,
};

pub trait PathExt {
    fn rm_rf(&self);
    fn mkdir_p(&self);
}

impl PathExt for Path {
    fn rm_rf(&self) {
        if !self.exists() {
            return;
        }

        for file in t!(fs::read_dir(self)) {
            let file = t!(file);
            if file.file_type().map(|m| m.is_dir()).unwrap_or(false) {
                file.path().rm_rf();
            } else {
                // On windows we can't remove a readonly file, and git will
                // often clone files as readonly. As a result, we have some
                // special logic to remove readonly files on windows.
                do_op(&file.path(), "remove file", |p| fs::remove_file(p));
            }
        }
        do_op(self, "remove dir", |p| fs::remove_dir(p));
    }

    fn mkdir_p(&self) {
        fs::create_dir_all(self).unwrap_or_else(|e| panic!("failed to mkdir_p {:?}: {}", self, e))
    }
}

fn do_op<F>(path: &Path, desc: &str, mut f: F)
where
    F: FnMut(&Path) -> io::Result<()>,
{
    match f(path) {
        Ok(()) => {}
        Err(ref e) if cfg!(windows) && e.kind() == ErrorKind::PermissionDenied => {
            let mut p = t!(path.metadata()).permissions();
            p.set_readonly(false);
            t!(fs::set_permissions(path, p));
            f(path).unwrap_or_else(|e| {
                panic!("failed to {} {}: {}", desc, path.display(), e);
            })
        }
        Err(e) => {
            panic!("failed to {} {}: {}", desc, path.display(), e);
        }
    }
}
