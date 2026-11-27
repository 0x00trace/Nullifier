//! The title's own `settings.xml`: where it is, reading and writing it, and how
//! each of its settings is shown.

use std::path::PathBuf;

use crate::settings::data_dir;

#[derive(Clone)]
pub(crate) struct GameSetting {
    pub(crate) section: String,
    pub(crate) key: String,
    pub(crate) value: String,
    pub(crate) line: usize,
    pub(crate) indent: String,
}

pub(crate) struct GameSettings {
    pub(crate) path: PathBuf,
    pub(crate) lines: Vec<String>,
    pub(crate) settings: Vec<GameSetting>,
    pub(crate) dirty: bool,
    pub(crate) note: String,
}

fn find_game_settings() -> Option<PathBuf> {
    let roots = [
        data_dir().map(|d| d.join("prefix/pfx/drive_c/users")),
        data_dir().map(|d| d.join("data")),
    ];
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for root in roots.into_iter().flatten() {
        let mut stack = vec![(root, 0usize)];
        let mut seen = 0usize;
        while let Some((dir, depth)) = stack.pop() {
            if depth > 12 || seen > 40_000 {
                continue;
            }
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                seen += 1;
                let path = entry.path();
                let Ok(kind) = entry.file_type() else { continue };
                if kind.is_dir() {
                    stack.push((path, depth + 1));
                } else if path.file_name().and_then(|n| n.to_str()) == Some("settings.xml")
                    && dir.ends_with("storage/settings")
                {
                    if let Ok(meta) = entry.metadata() {
                        let when = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
                        if best.as_ref().is_none_or(|(t, _)| when > *t) {
                            best = Some((when, path));
                        }
                    }
                }
            }
        }
    }
    best.map(|(_, p)| p)
}

pub(crate) fn load_game_settings() -> Option<GameSettings> {
    let path = find_game_settings()?;
    let text = std::fs::read_to_string(&path).ok()?;
    let lines: Vec<String> = text.lines().map(str::to_string).collect();
    let mut settings = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if !trimmed.starts_with('<') || trimmed.starts_with("<?") || trimmed.starts_with("<!") {
            continue;
        }
        let self_closing = trimmed.ends_with("/>");
        let name = trimmed
            .trim_start_matches('<')
            .trim_start_matches('/')
            .split(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .next()
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            continue;
        }
        if trimmed.starts_with("</") {
            if stack.last().map(String::as_str) == Some(name.as_str()) {
                stack.pop();
            }
            continue;
        }
        if let Some(value) = attribute(trimmed, "value") {
            let indent = line[..line.len() - line.trim_start().len()].to_string();
            let section = stack
                .iter()
                .skip(1)
                .cloned()
                .collect::<Vec<_>>()
                .join("/");
            settings.push(GameSetting {
                section,
                key: name.clone(),
                value,
                line: i,
                indent,
            });
            if !self_closing {
                stack.push(name);
            }
        } else if !self_closing && !trimmed.contains(&format!("</{name}>")) {
            stack.push(name);
        }
    }
    let note = format!("{} settings from settings.xml", settings.len());
    Some(GameSettings { path, lines, settings, dirty: false, note })
}

fn attribute(line: &str, name: &str) -> Option<String> {
    let at = line.find(&format!("{name}=\""))? + name.len() + 2;
    let rest = &line[at..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn pretty(key: &str) -> String {
    let mut out = String::new();
    for (i, c) in key.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push(' ');
        }
        out.push(c);
    }
    let mut chars = out.chars();
    let mut out = match chars.next() {
        Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    };
    for (from, to) in [
        ("Hdr", "HDR"), ("Ui", "UI"), ("U I", "UI"), ("Fps", "FPS"), ("Fsr", "FSR"),
        ("Msaa", "MSAA"), ("Ssro", "SSRO"), ("Vrs", "VRS"), ("S lod", "SLOD"),
    ] {
        out = out.replace(from, to);
    }
    out
}

pub(crate) fn section_title(section: &str) -> &'static str {
    match section {
        "graphics" => "Graphics",
        "advancedGraphics" => "Advanced graphics",
        "video" => "Video",
        "audio" => "Audio",
        "population" => "Population",
        "physics" => "Physics",
        "ai" => "AI",
        _ if section.ends_with("resolutionFullscreen") => "Fullscreen resolution",
        _ if section.ends_with("uiResolution") => "UI resolution",
        _ => "Other",
    }
}

/// The settings a player looks for first, in the order they normally appear in
/// a game's options. Everything else goes under Advanced graphics settings.
pub(crate) const DISPLAY_KEYS: &[(&str, &str)] = &[("advancedGraphics", "motionBlur")];

pub(crate) const GRAPHICS_KEYS: &[(&str, &str)] = &[
    ("graphics", "graphicsQualityPreset"),
    ("graphics", "hdr"),
    ("graphics", "gamma"),
    ("graphics", "anisotropicFiltering"),
    ("graphics", "fsrQuality"),
    ("advancedGraphics", "fsrSharpen"),
    ("advancedGraphics", "sharpenEnabled"),
    ("advancedGraphics", "sharpenIntensity"),
    ("advancedGraphics", "lodScale"),
    ("advancedGraphics", "grassLod"),
    ("advancedGraphics", "texLodScale_Global"),
    ("advancedGraphics", "treeTessellationEnabled"),
];

pub(crate) const SHADOW_KEYS: &[(&str, &str)] = &[
    ("advancedGraphics", "directionalScreenSpaceShadowQuality"),
    ("advancedGraphics", "worldHeightShadowQuality"),
    ("advancedGraphics", "waterReflectionResX"),
    ("advancedGraphics", "waterReflectionResY"),
    ("advancedGraphics", "waterReflectionSSR"),
];

pub(crate) fn is_curated(section: &str, key: &str) -> bool {
    DISPLAY_KEYS
        .iter()
        .chain(GRAPHICS_KEYS)
        .chain(SHADOW_KEYS)
        .any(|(s, k)| *s == section && *k == key)
        || (section == "video/resolutionFullscreen" && (key == "width" || key == "height"))
        || (section == "video" && (key == "vSync" || key == "frameRateLimit"))
}

pub(crate) fn apply_game(game: &mut GameSettings) -> std::io::Result<()> {
    let backup = game.path.with_file_name("settings.xml.bak");
    std::fs::copy(&game.path, &backup)?;
    let mut text = game.lines.join("\n");
    text.push('\n');
    std::fs::write(&game.path, text)
}

pub(crate) fn label_of(setting: &GameSetting) -> String {
    match (setting.section.as_str(), setting.key.as_str()) {
        ("video/resolutionFullscreen", "width") => "Fullscreen width".into(),
        ("video/resolutionFullscreen", "height") => "Fullscreen height".into(),
        ("video/uiResolution", "width") => "UI width".into(),
        ("video/uiResolution", "height") => "UI height".into(),
        (_, key) => pretty(key),
    }
}

/// Settings the title stores with decimals. Deciding by parsing would turn a
/// slider at 0 or 1 into an integer text box, because `1` parses as both.
pub(crate) fn float_key(key: &str) -> bool {
    matches!(
        key,
        "graphicsQualityPreset"
            | "directionalScreenSpaceShadowQuality"
            | "fsrSharpen"
            | "grassLod"
            | "lodScale"
            | "texLodScale_Global"
            | "texLodScale_Mat_High"
            | "texLodScale_Mat_Med"
            | "texLodScale_Mat_Low"
            | "texLodScale_Buildings"
            | "texLodScale_Objects"
            | "texLodScale_HD"
            | "texLodScale_OrphanHD"
            | "texLodScale_LOD"
            | "texLodScale_SLOD1"
            | "texLodScale_SLOD2"
            | "texLodScale_SLOD3"
            | "texLodScale_SLOD4"
            | "smallCullThreshold"
            | "motionBlurLimit"
            | "motionBlurScale"
            | "sharpenIntensity"
            | "vehicleLodBias"
            | "worldHeightShadowQuality"
            | "manualMipBias"
    )
}

pub(crate) fn float_range(key: &str) -> (f32, f32, f32) {
    match key {
        "fsrSharpen" | "motionBlurScale" => (0.0, 1.0, 0.01),
        "sharpenIntensity" => (0.0, 2.0, 0.05),
        "motionBlurLimit" => (0.0, 32.0, 1.0),
        "smallCullThreshold" => (0.0, 4.0, 0.05),
        "graphicsQualityPreset" => (0.0, 4.0, 1.0),
        "texLodScale_LOD" => (0.0, 4.0, 0.1),
        "texLodScale_SLOD1" => (0.0, 8.0, 0.1),
        "texLodScale_SLOD2" => (0.0, 16.0, 0.5),
        "texLodScale_SLOD3" => (0.0, 32.0, 0.5),
        "texLodScale_SLOD4" => (0.0, 64.0, 0.5),
        "lodScale"
        | "grassLod"
        | "vehicleLodBias"
        | "texLodScale_Global"
        | "texLodScale_Mat_Low"
        | "texLodScale_Buildings"
        | "texLodScale_Objects"
        | "texLodScale_HD"
        | "texLodScale_OrphanHD"
        | "directionalScreenSpaceShadowQuality"
        | "worldHeightShadowQuality" => (0.0, 2.0, 0.05),
        _ => limits(key)
            .map(|(min, max, step)| (min as f32, max as f32, step as f32))
            .unwrap_or((0.0, 1.0, 0.01)),
    }
}

/// Settings with a small, named set of values: a drop-down, never a slider.
pub(crate) fn choices(key: &str) -> Option<&'static [(&'static str, &'static str)]> {
    match key {
        "vSync" => Some(&[("Off", "0"), ("On", "1"), ("Adaptive", "2")]),
        "frameRateLimit" => Some(&[
            ("Unlimited", "0"),
            ("30 fps", "30"),
            ("60 fps", "60"),
            ("120 fps", "120"),
            ("144 fps", "144"),
            ("240 fps", "240"),
        ]),
        "desiredFPS" => Some(&[("30 fps", "30"), ("60 fps", "60"), ("120 fps", "120")]),
        "anisotropicFiltering" => Some(&[("Off", "0"), ("4×", "4"), ("8×", "8"), ("16×", "16")]),
        "fsrQuality" => Some(&[
            ("Off", "0"),
            ("Quality", "1"),
            ("Balanced", "2"),
            ("Performance", "3"),
        ]),
        "graphicsQualityPreset" => Some(&[
            ("Custom (0)", "0"),
            ("Low (1)", "1"),
            ("Medium (2)", "2"),
            ("High (3)", "3"),
            ("Ultra (4)", "4"),
        ]),
        "waterReflectionResX" => Some(&[
            ("128 px", "128"),
            ("256 px", "256"),
            ("512 px", "512"),
            ("1024 px", "1024"),
        ]),
        "waterReflectionResY" => Some(&[
            ("64 px", "64"),
            ("128 px", "128"),
            ("256 px", "256"),
            ("512 px", "512"),
        ]),
        _ => None,
    }
}

/// Whole-number ranges that read better as a slider than a text box.
pub(crate) fn int_slider(key: &str) -> Option<(i64, i64, &'static str)> {
    match key {
        "gamma" => Some((1, 50, "")),
        "hdrIntensity" | "hdrBlackLevels" => Some((0, 100, "")),
        "hdrPeakBrightness"
        | "hdrPeakBrightnessUIDefaultNits"
        | "hdrPeakBrightnessGameDefaultNits" => Some((100, 2000, " nits")),
        "decorationResolutionShift" | "lensFlaresScreenResDivExponent" => Some((0, 4, "")),
        "maxTexUpgradesPerFrame" => Some((0, 10, "")),
        "ssroSampleCount" | "vehicleHDCount" => Some((0, 8, "")),
        "adapterIndex" | "outputIndex" => Some((0, 4, "")),
        _ => None,
    }
}

pub(crate) fn limits(key: &str) -> Option<(f64, f64, f64)> {
    match key {
        "gamma" => Some((0.0, 50.0, 1.0)),
        "anisotropicFiltering" => Some((0.0, 16.0, 1.0)),
        "desiredFPS" | "frameRateLimit" => Some((0.0, 240.0, 1.0)),
        "vSync" => Some((0.0, 2.0, 1.0)),
        "fsrQuality" => Some((0.0, 4.0, 1.0)),
        "fsrSharpen" | "manualMipBias" => Some((-5.0, 5.0, 0.05)),
        "lodScale" | "grassLod" | "texLodScale_Global" | "texLodScale_LOD"
        | "texLodScale_SLOD1" | "texLodScale_SLOD2" | "texLodScale_SLOD3"
        | "texLodScale_SLOD4" | "texLodScale_Mat_High" | "texLodScale_Mat_Med"
        | "texLodScale_Mat_Low" | "texLodScale_Buildings" | "texLodScale_Objects"
        | "texLodScale_HD" | "texLodScale_OrphanHD" | "vehicleLodBias" => {
            Some((0.0, 64.0, 0.05))
        }
        "width" | "height" => Some((0.0, 7680.0, 1.0)),
        _ => None,
    }
}

/// The title ignores these in the current build. Measured on 2026-11-27
/// (docs/FINDINGS.md 3.29c): neither frame rate, draws nor copy-back moved;
/// only gamma is confirmed to change the picture.
pub(crate) fn no_effect(key: &str) -> bool {
    matches!(
        key,
        "motionBlur"
            | "rtDynamicShadows"
            | "rtLODLightShadows"
            | "lodScale"
            | "grassLod"
            | "fsrQuality"
            | "asyncComputeEnabled"
            | "waterReflectionSSR"
            | "shadowParticleShadows"
            | "shadowAlphaTintedShadows"
            | "graphicsQualityPreset"
    )
}
