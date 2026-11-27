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
    if cfg!(windows) {
        [
            root.join("target/release/nullifier.exe"),
            root.join("target/x86_64-pc-windows-msvc/diag/nullifier.exe"),
        ]
        .into_iter()
        .find(|path| path.is_file())
    } else {
        let path = root.join("target/x86_64-pc-windows-msvc/diag/nullifier.exe");
        path.is_file().then_some(path)
    }
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
