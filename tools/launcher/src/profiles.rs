//! The profiles in `launcher/profiles.json`, and the one that matches this machine.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use crate::hardware::Hardware;

pub(crate) struct Profile {
    pub(crate) name: String,
    pub(crate) sets: BTreeMap<String, String>,
    pub(crate) note: String,
    os: String,
    vendor: String,
    vram_min: u32,
    vram_max: u32,
    pub(crate) confidence: String,
}

pub(crate) fn profiles(root: &Path) -> Vec<Profile> {
    let text = std::fs::read_to_string(root.join("launcher/profiles.json")).unwrap_or_default();
    let doc: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    let defaults: BTreeMap<String, String> = doc
        .get("defaults")
        .and_then(|d| serde_json::from_value(d.clone()).ok())
        .unwrap_or_default();
    doc.get("profiles")
        .and_then(|p| p.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|p| {
                    let name = p.get("name")?.as_str()?.to_string();
                    let mut sets = defaults.clone();
                    if let Some(map) = p.get("sets").and_then(|s| s.as_object()) {
                        for (k, v) in map {
                            let v = v
                                .as_str()
                                .map(str::to_string)
                                .unwrap_or_else(|| v.to_string());
                            sets.insert(k.clone(), v);
                        }
                    }
                    let m = p.get("match");
                    let get = |key: &str| m.and_then(|m| m.get(key));
                    Some(Profile {
                        name,
                        sets,
                        note: p.get("note").and_then(|n| n.as_str()).unwrap_or("").to_string(),
                        os: get("os").and_then(|o| o.as_str()).unwrap_or("").to_string(),
                        vendor: get("vendor").and_then(|o| o.as_str()).unwrap_or("").to_string(),
                        vram_min: get("vram_min_mb").and_then(|o| o.as_u64()).unwrap_or(0) as u32,
                        vram_max: get("vram_max_mb")
                            .and_then(|o| o.as_u64())
                            .unwrap_or(u32::MAX as u64) as u32,
                        confidence: p
                            .get("confidence")
                            .and_then(|c| c.as_str())
                            .unwrap_or("")
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn os_name() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else {
        "linux"
    }
}

pub(crate) fn auto_profile(profiles: &[Profile], hw: &Hardware) -> String {
    if let Some(p) = profiles.iter().find(|p| {
        p.os == os_name()
            && (hw.vendor.is_empty() || p.vendor == hw.vendor || p.vendor.is_empty())
            && (hw.vram_mb == 0 || (hw.vram_mb >= p.vram_min && hw.vram_mb <= p.vram_max))
    }) {
        return p.name.clone();
    }
    profiles
        .iter()
        .find(|p| p.os == os_name())
        .or_else(|| profiles.first())
        .map(|p| p.name.clone())
        .unwrap_or_default()
}
