//! Where the tree, the engine build and the title's executable are, and whether
//! the engine and the wineserver are built.

use std::path::{Path, PathBuf};

use crate::settings::{data_dir, Settings};

pub(crate) fn find_root() -> Option<PathBuf> {
    let mut starts: Vec<PathBuf> = Vec::new();
    if let Some(root) = std::env::var_os("NULLIFIER_ROOT") {
        starts.push(PathBuf::from(root));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            starts.push(dir.to_path_buf());
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        starts.push(cwd);
    }
    starts.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."));
    for start in starts {
        for dir in start.ancestors() {
            if dir.join("launcher/profiles.json").is_file() {
                return Some(dir.to_path_buf());
            }
        }
    }
    None
}

pub(crate) fn launcher_path(root: &Path) -> PathBuf {
    if cfg!(windows) {
        root.join("launcher/nullifier-launch.ps1")
    } else {
        root.join("launcher/nullifier-launch.sh")
    }
}

pub(crate) fn settings_root_readiness(settings: &Settings) -> Readiness {
    settings.root.as_deref().map(readiness).unwrap_or_default()
}

/// The engine binary a run or a preload starts.
pub(crate) fn engine_exe(root: &Path) -> Option<PathBuf> {
    engine_candidates(root)
        .into_iter()
        .find(|path| path.is_file())
}

/// Where the engine can be, best first. A diag build on Windows lands in
/// `target/diag`; the `target/<triple>/diag` path is the one a cross build from
/// Linux makes (`tools/setup.sh`). A tree can hold both, so between them the
/// file built last wins: an older diag binary would otherwise run unnoticed.
fn engine_candidates(root: &Path) -> Vec<PathBuf> {
    if !cfg!(windows) {
        return vec![root.join("target/x86_64-pc-windows-msvc/diag/nullifier.exe")];
    }
    let mut out = vec![root.join("target/release/nullifier.exe")];
    let native = root.join("target/diag/nullifier.exe");
    let cross = root.join("target/x86_64-pc-windows-msvc/diag/nullifier.exe");
    match (native.is_file(), cross.is_file()) {
        (true, true) => {
            let newer = if modified(&native) >= modified(&cross) {
                native
            } else {
                cross
            };
            out.push(newer);
        }
        (true, false) => out.push(native),
        (false, true) => out.push(cross),
        (false, false) => {}
    }
    out
}

fn modified(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
}

/// What still has to be built or installed before Play can work.
pub(crate) fn readiness(root: &Path) -> Readiness {
    let engine = engine_exe(root).is_some();
    // The built wineserver is a Linux/Wine thing; Windows uses its own server.
    let wineserver = cfg!(windows)
        || data_dir()
            .map(|d| d.join("wineserver/bin/wineserver").is_file())
            .unwrap_or(false);
    Readiness { engine, wineserver }
}

/// The pipeline cache the next run loads: how many title pipelines and how
/// many bytes of the host library.
pub(crate) fn pipeline_cache() -> Option<(usize, u64)> {
    let dir = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(|dir| PathBuf::from(dir).join("nullifier/cache/pipelines"))?
    } else {
        data_dir()?
            .join("prefix/pfx/drive_c/users/steamuser/AppData/Local/nullifier/cache/pipelines")
    };
    let list = std::fs::read_to_string(dir.join("early-pipelines.txt")).ok()?;
    let count = list.lines().filter(|line| !line.trim().is_empty()).count();
    let bytes = std::fs::metadata(dir.join("host-pipelines.bin"))
        .map(|meta| meta.len())
        .unwrap_or(0);
    Some((count, bytes))
}

/// The title's executable. Looks two levels down: dumps often keep one folder
/// inside the picked one.
pub(crate) fn title_exe(dir: &Path) -> Option<PathBuf> {
    const WANT: &str = "game_scarlett_master.exe";
    let mut stack = vec![(dir.to_path_buf(), 0usize)];
    while let Some((at, depth)) = stack.pop() {
        if depth > 2 {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&at) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                stack.push((path, depth + 1));
            } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                let lower = name.to_lowercase();
                if lower == WANT {
                    return Some(path);
                }
            }
        }
    }
    None
}

#[derive(Default, Clone, Copy)]
pub(crate) struct Readiness {
    pub(crate) engine: bool,
    pub(crate) wineserver: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[cfg(windows)]
    use std::time::{Duration, SystemTime};

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!(
                "nullifier-install-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::SeqCst)
            ));
            // A stopped run can leave a folder behind under a reused process id.
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn build(&self, relative: &str) {
            let path = self.0.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"stub").unwrap();
        }

        #[cfg(windows)]
        fn stamp(&self, relative: &str, at: SystemTime) {
            let path = self.0.join(relative);
            std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap()
                .set_modified(at)
                .unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn no_engine_built_is_none() {
        let tmp = TempDir::new();
        assert_eq!(engine_exe(tmp.path()), None);
    }

    #[cfg(windows)]
    #[test]
    fn a_native_diag_build_is_the_engine() {
        let tmp = TempDir::new();
        tmp.build("target/diag/nullifier.exe");
        assert_eq!(
            engine_exe(tmp.path()),
            Some(tmp.path().join("target/diag/nullifier.exe"))
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_cross_built_diag_is_the_engine_too() {
        let tmp = TempDir::new();
        tmp.build("target/x86_64-pc-windows-msvc/diag/nullifier.exe");
        assert_eq!(
            engine_exe(tmp.path()),
            Some(
                tmp.path()
                    .join("target/x86_64-pc-windows-msvc/diag/nullifier.exe")
            )
        );
    }

    #[cfg(windows)]
    #[test]
    fn the_newer_of_two_diag_builds_wins() {
        let tmp = TempDir::new();
        tmp.build("target/diag/nullifier.exe");
        tmp.build("target/x86_64-pc-windows-msvc/diag/nullifier.exe");
        let native = tmp.path().join("target/diag/nullifier.exe");
        let cross = tmp
            .path()
            .join("target/x86_64-pc-windows-msvc/diag/nullifier.exe");

        tmp.stamp(
            "target/diag/nullifier.exe",
            SystemTime::UNIX_EPOCH + Duration::from_secs(1_000),
        );
        tmp.stamp(
            "target/x86_64-pc-windows-msvc/diag/nullifier.exe",
            SystemTime::UNIX_EPOCH + Duration::from_secs(2_000),
        );
        assert_eq!(engine_exe(tmp.path()), Some(cross));

        tmp.stamp(
            "target/diag/nullifier.exe",
            SystemTime::UNIX_EPOCH + Duration::from_secs(3_000),
        );
        assert_eq!(engine_exe(tmp.path()), Some(native));
    }

    #[cfg(windows)]
    #[test]
    fn the_release_build_comes_first() {
        let tmp = TempDir::new();
        tmp.build("target/diag/nullifier.exe");
        tmp.build("target/release/nullifier.exe");
        assert_eq!(
            engine_exe(tmp.path()),
            Some(tmp.path().join("target/release/nullifier.exe"))
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn other_platforms_only_look_at_the_cross_build() {
        let tmp = TempDir::new();
        tmp.build("target/release/nullifier.exe");
        tmp.build("target/diag/nullifier.exe");
        assert_eq!(engine_exe(tmp.path()), None);
        tmp.build("target/x86_64-pc-windows-msvc/diag/nullifier.exe");
        assert_eq!(
            engine_exe(tmp.path()),
            Some(
                tmp.path()
                    .join("target/x86_64-pc-windows-msvc/diag/nullifier.exe")
            )
        );
    }
}
