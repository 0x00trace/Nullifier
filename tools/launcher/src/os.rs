//! Opening a link or a folder with the desktop's handler, and the home folder.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub(crate) fn open_url(url: &str) {
    open_path(Path::new(url));
}

pub(crate) fn open_path(path: &Path) {
    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.arg("/C").arg("start").arg("").arg(path);
        c
    } else {
        let mut c = Command::new("xdg-open");
        c.arg(path);
        c
    };
    let _ = cmd.stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}

pub(crate) fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}
