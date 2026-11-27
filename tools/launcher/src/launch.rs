//! The command that starts the title: the profile file, the environment and the
//! launcher script.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::json;

use crate::install::launcher_path;
use crate::profiles::os_name;
use crate::settings::{cache_dir, Settings};
use crate::{TITLE, VERSION};

pub(crate) fn custom_profile(settings: &Settings) -> Option<PathBuf> {
    let dir = cache_dir()?;
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join("launcher-profile.json");
    let doc = json!({
        "version": 1,
        "_comment": ["written by the Nullifier launcher on every run"],
        "defaults": {},
        "profiles": [{
            "name": "custom",
            "match": { "os": os_name(), "vendor": "*" },
            "confidence": "custom",
            "sets": settings.sets,
            "note": "from the Nullifier launcher"
        }]
    });
    let mut file = std::fs::File::create(&path).ok()?;
    file.write_all(serde_json::to_string_pretty(&doc).ok()?.as_bytes())
        .ok()?;
    Some(path)
}

fn extra_env(settings: &Settings) -> Vec<(String, String)> {
    settings
        .extra_env
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (k, v) = line.split_once('=')?;
            let k = k.trim();
            (!k.is_empty()).then(|| (k.to_string(), v.trim().to_string()))
        })
        .collect()
}

pub(crate) fn launch_command(root: &Path, settings: &Settings, profile_file: &Path) -> Command {
    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("powershell");
        c.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(launcher_path(root))
            .arg("-Game")
            .arg(&settings.game_dir)
            .arg("-Profile")
            .arg("custom");
        c
    } else {
        let mut c = Command::new("sh");
        c.arg(launcher_path(root))
            .arg("--game")
            .arg(&settings.game_dir)
            .arg("--profile")
            .arg("custom");
        c
    };
    cmd.env("VICTORY_PROFILES", profile_file);
    if !settings.log_level.trim().is_empty() {
        cmd.env("VICTORY_LOG", settings.log_level.trim());
    }
    if !settings.capture_dir.trim().is_empty() {
        cmd.env("VICTORY_CAPTURE", capture_path(&settings.capture_dir));
    }
    if !settings.capture_frames.trim().is_empty() {
        cmd.env("VICTORY_CAPTURE_FRAMES", settings.capture_frames.trim());
    }
    if !settings.keys.trim().is_empty() {
        cmd.env("VICTORY_KEYS", settings.keys.trim());
    }
    if !settings.keyboard.trim().is_empty() {
        cmd.env("VICTORY_KEYBOARD", settings.keyboard.trim());
    }
    if !settings.output.trim().is_empty() {
        cmd.env("VICTORY_OUTPUT", settings.output.trim());
    }
    if settings.render_scale > 0.0 {
        cmd.env(
            "VICTORY_RENDER_SCALE",
            format!("{:.2}", settings.render_scale),
        );
    }
    if settings.input_mode != "auto" {
        cmd.env("VICTORY_INPUT", settings.input_mode.trim());
    }
    if settings.mouse_look {
        cmd.env("VICTORY_MOUSE_LOOK", "1");
        cmd.env(
            "VICTORY_MOUSE_SENS",
            format!("{:.3}", settings.mouse_sensitivity),
        );
        if settings.mouse_invert {
            cmd.env("VICTORY_MOUSE_INVERT", "1");
        }
        if !settings.mouse_buttons {
            cmd.env("VICTORY_MOUSE_BUTTONS", "0");
        }
    }
    for (k, v) in extra_env(settings) {
        cmd.env(k, v);
    }
    cmd
}

/// The title sees Windows paths; a Linux path becomes `Z:\...`.
fn capture_path(dir: &str) -> String {
    if cfg!(windows) {
        dir.to_string()
    } else {
        format!("Z:{}", dir.replace('/', "\\"))
    }
}

pub(crate) fn check(root: Option<&Path>, settings: &Settings) {
    println!("{TITLE} {VERSION}");
    match root {
        Some(root) => println!("repo: {}", root.display()),
        None => println!("repo: not found (set NULLIFIER_ROOT)"),
    }
    println!("game: {}", settings.game_dir);
    println!("profile: {}", settings.profile);
    println!("sets:");
    for (k, v) in &settings.sets {
        println!("  {k}={v}");
    }
    if let Some(root) = root {
        if let Some(profile) = custom_profile(settings) {
            println!("profile file: {}", profile.display());
            let cmd = launch_command(root, settings, &profile);
            println!(
                "command: {:?} {:?}",
                cmd.get_program(),
                cmd.get_args().collect::<Vec<_>>()
            );
        }
    }
    println!(
        "planned (not applied): {}",
        serde_json::to_string(&settings.planned).unwrap()
    );
}

pub(crate) fn kill_game() {
    if cfg!(windows) {
        let _ = Command::new("taskkill")
            .args(["/F", "/IM", "nullifier.exe"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    } else {
        let _ = Command::new("pkill")
            .args(["-f", "nullifier[.]exe run"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}
