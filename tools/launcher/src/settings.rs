//! The launcher's saved settings, and the folders it keeps them and its data in.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::hardware::hardware;
use crate::profiles::{auto_profile, profiles};

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Settings {
    pub(crate) game_dir: String,
    /// The profile the sets came from. Empty means the auto-detected one.
    pub(crate) profile: String,
    /// The exact set of VICTORY_* a run gets.
    pub(crate) sets: BTreeMap<String, String>,
    pub(crate) log_level: String,
    pub(crate) capture_dir: String,
    pub(crate) capture_frames: String,
    pub(crate) keys: String,
    /// `KEY=VALUE` lines, applied last.
    pub(crate) extra_env: String,
    /// FNV-1a of launcher/profiles.json when the sets were last taken from it.
    #[serde(default)]
    pub(crate) profiles_hash: u64,
    #[serde(default)]
    pub(crate) planned: Planned,
    /// Show the technical tabs (Rendering, Advanced, Tools).
    #[serde(default)]
    pub(crate) show_advanced: bool,
    /// `Esc=Menu,Enter=A,...` for the keyboard stand-in pad. Empty = defaults.
    #[serde(default)]
    pub(crate) keyboard: String,
    /// `VICTORY_INPUT`: "auto", "controller" or "keyboard".
    #[serde(default = "default_input_mode")]
    pub(crate) input_mode: String,
    #[serde(default)]
    pub(crate) mouse_look: bool,
    #[serde(default = "default_mouse_sensitivity")]
    pub(crate) mouse_sensitivity: f32,
    #[serde(default)]
    pub(crate) mouse_invert: bool,
    /// Left/right buttons act as the right/left trigger.
    #[serde(default = "default_true")]
    pub(crate) mouse_buttons: bool,
    /// The name shown next to shared runs.
    #[serde(default)]
    pub(crate) community_name: String,
    /// Where the run was (a world section, a mission, a menu).
    #[serde(default)]
    pub(crate) community_area: String,
    /// What looked wrong or different.
    #[serde(default)]
    pub(crate) community_notes: String,
    /// `VICTORY_OUTPUT`: "", "2560x1440" or "1920x1080".
    #[serde(default)]
    pub(crate) output: String,
    /// `VICTORY_RENDER_SCALE`: 0.0 leaves the title's own 2/3.
    #[serde(default)]
    pub(crate) render_scale: f32,
    #[serde(skip)]
    pub(crate) root: Option<PathBuf>,
    #[serde(skip)]
    pub(crate) reloaded_profile: bool,
}

/// Toggles that need engine work; kept so the choice survives, never applied.
#[derive(Clone, Default, Serialize, Deserialize)]
pub(crate) struct Planned {
    pub(crate) fsr: bool,
    pub(crate) ray_tracing: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            game_dir: std::env::var("VICTORY_GAME_DIR").unwrap_or_default(),
            profile: String::new(),
            sets: BTreeMap::new(),
            log_level: "info".into(),
            capture_dir: String::new(),
            capture_frames: "1024,1536".into(),
            keys: String::new(),
            extra_env: String::new(),
            profiles_hash: 0,
            planned: Planned::default(),
            show_advanced: false,
            keyboard: String::new(),
            input_mode: default_input_mode(),
            mouse_look: false,
            mouse_sensitivity: 0.02,
            mouse_invert: false,
            mouse_buttons: true,
            community_name: String::new(),
            community_area: String::new(),
            community_notes: String::new(),
            output: String::new(),
            render_scale: 0.0,
            root: None,
            reloaded_profile: false,
        }
    }
}

impl Settings {
    pub(crate) fn load(root: Option<&Path>) -> Self {
        let mut settings: Settings = config_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        if let Some(root) = root {
            let hash = profile_hash(root);
            if settings.sets.is_empty() {
                let profiles = profiles(root);
                let hw = hardware(root);
                let name = auto_profile(&profiles, &hw);
                settings.profile = name.clone();
                settings.sets = profiles
                    .iter()
                    .find(|p| p.name == name)
                    .map(|p| p.sets.clone())
                    .unwrap_or_default();
                settings.profiles_hash = hash;
            } else if settings.profiles_hash != hash {
                // The profiles changed since these sets were taken (a new
                // measured switch, a new default): take the named profile
                // again rather than keep running old settings.
                let profiles = profiles(root);
                if let Some(profile) = profiles.iter().find(|p| p.name == settings.profile) {
                    settings.sets = profile.sets.clone();
                    settings.reloaded_profile = true;
                }
                settings.profiles_hash = hash;
            }
        }
        settings
    }

    pub(crate) fn save(&self) {
        let Some(path) = config_path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, text);
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_input_mode() -> String {
    "auto".into()
}

fn default_mouse_sensitivity() -> f32 {
    0.02
}

pub(crate) fn profile_hash(root: &Path) -> u64 {
    let bytes = std::fs::read(root.join("launcher/profiles.json")).unwrap_or_default();
    bytes.into_iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
    })
}

fn config_path() -> Option<PathBuf> {
    Some(user_dir()?.join("launcher.json"))
}

fn user_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("nullifier"))
    } else {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("nullifier"))
    }
}

pub(crate) fn data_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("nullifier"))
    } else {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
        Some(base.join("nullifier"))
    }
}

pub(crate) fn cache_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("nullifier"))
    } else {
        let base = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
        Some(base.join("nullifier"))
    }
}
