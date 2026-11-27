//! The card and its memory, as the launcher script detects them.

use std::path::Path;
use std::process::Command;

use crate::install::launcher_path;

pub(crate) fn vram_label(mb: u32) -> String {
    if mb == 0 {
        "unknown VRAM".into()
    } else if mb >= 1024 {
        format!("{:.0} GB", mb as f32 / 1024.0)
    } else {
        format!("{mb} MB")
    }
}

#[derive(Default, Clone)]
pub(crate) struct Hardware {
    pub(crate) vendor: String,
    pub(crate) vram_mb: u32,
    profile: String,
}

pub(crate) fn hardware(root: &Path) -> Hardware {
    let raw = detect(root);
    let mut hw = Hardware {
        ..Default::default()
    };
    for line in raw.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("GPU vendor") {
            hw.vendor = rest.trim().to_lowercase();
        } else if let Some(rest) = line.strip_prefix("VRAM") {
            hw.vram_mb = rest
                .split_whitespace()
                .next()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
        } else if let Some(rest) = line.strip_prefix("profile") {
            hw.profile = rest.split_whitespace().next().unwrap_or("").to_string();
        }
    }
    hw
}

pub(crate) fn detect(root: &Path) -> String {
    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("powershell");
        c.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(launcher_path(root))
            .arg("-Detect");
        c
    } else {
        let mut c = Command::new("sh");
        c.arg(launcher_path(root)).arg("--detect");
        c
    };
    cmd.env_remove("VICTORY_PROFILES");
    match cmd.output() {
        Ok(out) => format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
        Err(e) => format!("cannot run the launcher: {e}"),
    }
}
