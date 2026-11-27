//! Key names for the keyboard stand-in pad, and the controllers the system lists.

use std::path::PathBuf;
use std::process::Command;

use eframe::egui;

/// The stand-in pad's buttons, in the order the Controls page shows them.
pub(crate) const PAD_ACTIONS: [(&str, &str); 14] = [
    ("Menu", "Esc"),
    ("View", "Tab"),
    ("A", "Enter"),
    ("B", "Backspace"),
    ("X", "X"),
    ("Y", "Y"),
    ("Up", "Up"),
    ("Down", "Down"),
    ("Left", "Left"),
    ("Right", "Right"),
    ("LB", "Q"),
    ("RB", "E"),
    ("LS", "F"),
    ("RS", "H"),
];

pub(crate) fn load_keys(text: &str) -> Vec<String> {
    let mut keys: Vec<String> = PAD_ACTIONS.iter().map(|(_, key)| (*key).to_string()).collect();
    for pair in text.split([',', ';']) {
        let Some((key, action)) = pair.split_once('=') else { continue };
        if let Some(index) = PAD_ACTIONS
            .iter()
            .position(|(name, _)| name.eq_ignore_ascii_case(action.trim()))
        {
            keys[index] = key.trim().to_string();
        }
    }
    keys
}

pub(crate) fn keys_to_text(keys: &[String]) -> String {
    PAD_ACTIONS
        .iter()
        .zip(keys)
        .map(|((action, _), key)| format!("{}={}", key.trim(), action))
        .collect::<Vec<_>>()
        .join(",")
}

/// The key's name as the keyboard bindings spell it.
pub(crate) fn key_name(key: egui::Key) -> String {
    let name = format!("{key:?}");
    match name.as_str() {
        "ArrowUp" => "Up".into(),
        "ArrowDown" => "Down".into(),
        "ArrowLeft" => "Left".into(),
        "ArrowRight" => "Right".into(),
        "Escape" => "Esc".into(),
        "Backspace" => "Backspace".into(),
        "Enter" => "Enter".into(),
        "Space" => "Space".into(),
        "Tab" => "Tab".into(),
        other => other.to_string(),
    }
}

pub(crate) fn same_key(bound: &str, key: &str) -> bool {
    let bound = bound.trim().to_ascii_lowercase();
    let key = key.trim().to_ascii_lowercase();
    let alias = |name: &str| match name {
        "escape" => "esc".to_string(),
        "return" => "enter".to_string(),
        "back" => "backspace".to_string(),
        other => other.to_string(),
    };
    alias(&bound) == alias(&key)
}

pub(crate) fn key_name_ok(name: &str) -> bool {
    let name = name.trim().to_ascii_lowercase();
    if matches!(
        name.as_str(),
        "esc" | "escape" | "tab" | "enter" | "return" | "backspace" | "back" | "space"
            | "up" | "down" | "left" | "right" | "shift" | "ctrl" | "control" | "alt"
    ) {
        return true;
    }
    if name.len() == 1 {
        return name.as_bytes()[0].is_ascii_alphanumeric();
    }
    name.strip_prefix('f')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=12).contains(&n))
}

/// Connected pads, as the system names them. Soft errors are just "none".
pub(crate) fn scan_controllers() -> Vec<String> {
    let mut names = Vec::new();
    if cfg!(windows) {
        let out = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Get-CimInstance Win32_PnPEntity | Where-Object { $_.Name -match 'Xbox|DualSense|DualShock|Wireless Controller|Gamepad' } | ForEach-Object { $_.Name }",
            ])
            .output();
        if let Ok(out) = out {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                let line = line.trim();
                if !line.is_empty() {
                    names.push(line.to_string());
                }
            }
        }
    } else {
        let root = std::env::var_os("NULLIFIER_INPUT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/sys/class/input"));
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                if !name.to_string_lossy().starts_with("event") {
                    continue;
                }
                let path = entry.path().join("device/name");
                if let Ok(text) = std::fs::read_to_string(path) {
                    let text = text.trim().to_string();
                    let lower = text.to_lowercase();
                    if [
                        "xbox",
                        "dualsense",
                        "dualshock",
                        "controller",
                        "gamepad",
                        "sony interactive",
                    ]
                    .iter()
                    .any(|needle| lower.contains(needle))
                    {
                        names.push(text);
                    }
                }
            }
        }
    }
    names.sort();
    names.dedup();
    names
}
