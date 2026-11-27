//! The check for a newer release.

use std::process::Command;

use crate::settings::cache_dir;

pub(crate) const RELEASES_URL: &str = "https://github.com/iNiKKo/Nullifier/releases/latest";

fn version_parts(text: &str) -> Vec<u32> {
    text.trim()
        .trim_start_matches('v')
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.parse().ok())
        .collect()
}

pub(crate) fn is_newer(latest: &str, current: &str) -> bool {
    let a = version_parts(latest);
    let b = version_parts(current);
    for i in 0..a.len().max(b.len()) {
        let x = *a.get(i).unwrap_or(&0);
        let y = *b.get(i).unwrap_or(&0);
        if x != y {
            return x > y;
        }
    }
    false
}

/// A GitHub JSON endpoint as text, or nothing when offline.
fn fetch_json(url: &str) -> Option<String> {
    let out = if cfg!(windows) {
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "try {{ Invoke-RestMethod -Uri '{url}' -TimeoutSec 6 | ConvertTo-Json -Compress }} catch {{ }}"
                ),
            ])
            .output()
    } else {
        Command::new("curl")
            .args([
                "-fsSL",
                "--max-time",
                "6",
                "-H",
                "Accept: application/vnd.github+json",
                url,
            ])
            .output()
    }
    .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn tag_from(text: &str) -> Option<String> {
    if let Some(at) = text.find("\"tag_name\"") {
        let rest = &text[at..];
        let start = rest.find(':')? + 1;
        let rest = rest[start..].trim_start().strip_prefix('"')?;
        let end = rest.find('"')?;
        return Some(rest[..end].trim().to_string());
    }
    if let Some(at) = text.find("\"name\"") {
        let rest = &text[at..];
        let start = rest.find(':')? + 1;
        let rest = rest[start..].trim_start().strip_prefix('"')?;
        let end = rest.find('"')?;
        let name = rest[..end].trim();
        return name.starts_with('v').then(|| name.to_string());
    }
    let line = text.lines().map(str::trim).find(|line| {
        line.starts_with('v') || line.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
    })?;
    Some(line.to_string())
}

/// The newest release tag, falling back to tags when no release is published.
fn fetch_latest_tag() -> Option<String> {
    let base = "https://api.github.com/repos/iNiKKo/Nullifier";
    fetch_json(&format!("{base}/releases/latest"))
        .and_then(|text| tag_from(&text))
        .or_else(|| fetch_json(&format!("{base}/tags")).and_then(|text| tag_from(&text)))
}

/// Once a day at most, in the background. A cached answer is used as is.
pub(crate) fn update_check() -> (Option<String>, Option<std::sync::mpsc::Receiver<Option<String>>>) {
    if std::env::var_os("NULLIFIER_NO_UPDATE_CHECK").is_some() {
        return (None, None);
    }
    let Some(cache) = cache_dir().map(|dir| dir.join("update.txt")) else {
        return (None, None);
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if let Ok(text) = std::fs::read_to_string(&cache) {
        let mut lines = text.lines();
        let tag = lines.next().unwrap_or("").trim().to_string();
        let when: u64 = lines.next().unwrap_or("0").trim().parse().unwrap_or(0);
        if !tag.is_empty() && now.saturating_sub(when) < 86_400 {
            return (Some(tag), None);
        }
    }
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let tag = fetch_latest_tag();
        if let Some(tag) = &tag {
            if let Some(dir) = cache.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(&cache, format!("{tag}\n{now}\n"));
        }
        let _ = tx.send(tag);
    });
    (None, Some(rx))
}
