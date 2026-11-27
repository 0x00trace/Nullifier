//! Nullifier's graphical launcher.
//!
//! A rail of four pages, one primary button:
//!   Play      - the game folder, the auto-detected profile and one big Launch,
//!               with the few settings a player touches next to it.
//!   Graphics  - the measured switches, in plain words.
//!   Advanced  - every switch, free-form environment, and what is not built.
//!   Debug     - detect, logging, captures, scripted keys and a benchmark.
//!
//! The launchers honour `VICTORY_PROFILES`, so this front end writes an exact
//! profile and forces it: a switch that is off is absent, never `=0`, and no
//! reader that tests for presence can be fooled.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use eframe::egui;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const TITLE: &str = "Nullifier";
const ACCENT: egui::Color32 = egui::Color32::from_rgb(77, 122, 235);
const VERSION: &str = env!("CARGO_PKG_VERSION");
/// The translation layer's build string. Kept beside `BUILD` in
/// `crates/victory-gpu/src/host.rs` (bumped by hand there).
const LAYER_VERSION: &str = "0.005";


fn apply_theme(ctx: &egui::Context) {
    use egui::{Color32, FontFamily, FontId, Stroke, TextStyle};

    // Shared visual tokens. Keep the palette deliberately restrained so the
    // launcher feels at home on KDE while remaining consistent on Windows.
    let bg = Color32::from_rgb(23, 25, 31);
    let surface = Color32::from_rgb(34, 38, 48);
    let raised = Color32::from_rgb(41, 46, 57);
    let border = Color32::from_rgb(52, 58, 71);
    let text = Color32::from_rgb(230, 233, 240);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(13.0, 8.0);
    style.spacing.interact_size = egui::vec2(40.0, 34.0);
    style.spacing.indent = 18.0;
    style.spacing.window_margin = egui::Margin::same(18);
    style.text_styles = [
        (TextStyle::Heading, FontId::new(23.0, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Small, FontId::new(11.5, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(12.5, FontFamily::Monospace)),
    ].into();

    let mut v = egui::Visuals::dark();
    v.panel_fill = bg;
    v.window_fill = surface;
    v.faint_bg_color = raised;
    v.extreme_bg_color = Color32::from_rgb(19, 21, 26);
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, text);
    v.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, text);
    v.widgets.hovered.fg_stroke = egui::Stroke::new(1.0_f32, text);
    v.widgets.active.fg_stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);
    v.widgets.open.fg_stroke = egui::Stroke::new(1.0_f32, text);
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = Color32::from_rgb(54, 83, 143);
    v.selection.stroke = Stroke::new(1.0_f32, Color32::from_rgb(132, 164, 235));
    v.window_corner_radius = 10.into();
    v.menu_corner_radius = 8.into();
    v.window_stroke = Stroke::new(1.0_f32, border);
    v.popup_shadow = egui::Shadow::NONE;

    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = 7.into();
    }
    v.widgets.noninteractive.bg_fill = surface;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, border);
    v.widgets.inactive.bg_fill = raised;
    v.widgets.inactive.weak_bg_fill = surface;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, border);
    v.widgets.hovered.bg_fill = Color32::from_rgb(49, 56, 69);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(49, 56, 69);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(76, 88, 108));
    v.widgets.active.bg_fill = ACCENT;
    v.widgets.active.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(111, 151, 246));
    v.widgets.open.bg_fill = raised;
    v.widgets.open.bg_stroke = Stroke::new(1.0_f32, border);

    style.visuals = v;
    ctx.set_style(style);

}

fn main() -> eframe::Result<()> {
    let root = find_root();
    let mut settings = Settings::load(root.as_deref());

    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--game-list") {
        match load_game_settings() {
            Some(game) => {
                println!("# {}", game.path.display());
                for s in &game.settings {
                    println!("{}/{}={}", s.section, s.key, s.value);
                }
            }
            None => println!("no settings.xml found"),
        }
        return Ok(());
    }
    let mut changes: Vec<(String, String)> = Vec::new();
    for i in 0..args.len() {
        if args[i] == "--game-set" {
            if let Some((k, v)) = args.get(i + 1).and_then(|a| a.split_once('=')) {
                changes.push((k.to_string(), v.to_string()));
            }
        }
    }
    if !changes.is_empty() {
        let Some(mut game) = load_game_settings() else {
            eprintln!("no settings.xml found");
            return Ok(());
        };
        let mut ok = true;
        for (name, value) in changes {
            let found = game.settings.iter_mut().find(|s| {
                let full = format!("{}/{}", s.section, s.key);
                full == name || s.key == name || full.ends_with(&format!("/{name}"))
            });
            match found {
                Some(setting) => {
                    setting.value = value.clone();
                    let line = format!(
                        "{}<{} value=\"{}\" />",
                        setting.indent, setting.key, setting.value
                    );
                    game.lines[setting.line] = line;
                    println!("{name}={value}");
                }
                None => {
                    eprintln!("no such setting: {name}");
                    ok = false;
                }
            }
        }
        if ok {
            match apply_game(&mut game) {
                Ok(()) => println!("saved (settings.xml.bak kept)"),
                Err(e) => eprintln!("could not save: {e}"),
            }
        }
        return Ok(());
    }
    if args.iter().any(|a| a == "--check") {
        check(root.as_deref(), &settings);
        return Ok(());
    }
    let start_tab = std::env::args()
        .position(|a| a == "--tab")
        .and_then(|i| std::env::args().nth(i + 1));
    let start_info = std::env::args()
        .position(|a| a == "--info")
        .and_then(|i| std::env::args().nth(i + 1));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1040.0, 720.0])
            .with_min_inner_size([820.0, 560.0])
            .with_title(TITLE),
        ..Default::default()
    };
    eframe::run_native(
        TITLE,
        options,
        Box::new(move |cc| {
            apply_theme(&cc.egui_ctx);
            settings.root = root.clone();
            let mut app = App::new(settings);
            app.tab = match start_tab.as_deref() {
                Some("game") => Tab::Graphics,
                Some("graphics") => Tab::Graphics,
                Some("controls") => Tab::Controls,
                Some("community") => Tab::Community,
                Some("rendering") => Tab::Rendering,
                Some("advanced") => Tab::Advanced,
                Some("debug") | Some("tools") => Tab::Debug,
                _ => Tab::Play,
            };
            if matches!(app.tab, Tab::Rendering | Tab::Advanced | Tab::Debug) {
                app.settings.show_advanced = true;
            }
            app.info = start_info;
            if std::env::args().any(|a| a == "--bench") {
                app.bench_now();
            }
            Ok(Box::new(app))
        }),
    )
}

// ---------------------------------------------------------------- settings

#[derive(Clone, Serialize, Deserialize)]
struct Settings {
    game_dir: String,
    /// The profile the sets came from. Empty means the auto-detected one.
    profile: String,
    /// The exact set of VICTORY_* a run gets.
    sets: BTreeMap<String, String>,
    log_level: String,
    capture_dir: String,
    capture_frames: String,
    keys: String,
    /// `KEY=VALUE` lines, applied last.
    extra_env: String,
    /// FNV-1a of launcher/profiles.json when the sets were last taken from it.
    #[serde(default)]
    profiles_hash: u64,
    #[serde(default)]
    planned: Planned,
    /// Show the technical tabs (Rendering, Advanced, Tools).
    #[serde(default)]
    show_advanced: bool,
    /// `Esc=Menu,Enter=A,...` for the keyboard stand-in pad. Empty = defaults.
    #[serde(default)]
    keyboard: String,
    /// `VICTORY_INPUT`: "auto", "controller" or "keyboard".
    #[serde(default = "default_input_mode")]
    input_mode: String,
    #[serde(default)]
    mouse_look: bool,
    #[serde(default = "default_mouse_sensitivity")]
    mouse_sensitivity: f32,
    #[serde(default)]
    mouse_invert: bool,
    /// Left/right buttons act as the right/left trigger.
    #[serde(default = "default_true")]
    mouse_buttons: bool,
    /// The name shown next to shared runs.
    #[serde(default)]
    community_name: String,
    /// Where the run was (a world section, a mission, a menu).
    #[serde(default)]
    community_area: String,
    /// What looked wrong or different.
    #[serde(default)]
    community_notes: String,
    /// `VICTORY_OUTPUT`: "", "2560x1440" or "1920x1080".
    #[serde(default)]
    output: String,
    /// `VICTORY_RENDER_SCALE`: 0.0 leaves the title's own 2/3.
    #[serde(default)]
    render_scale: f32,
    #[serde(skip)]
    root: Option<PathBuf>,
    #[serde(skip)]
    reloaded_profile: bool,
}

/// Toggles that need engine work; kept so the choice survives, never applied.
#[derive(Clone, Default, Serialize, Deserialize)]
struct Planned {
    fsr: bool,
    ray_tracing: bool,
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
    fn load(root: Option<&Path>) -> Self {
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

    fn save(&self) {
        let Some(path) = config_path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, text);
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
struct CommunityEntry {
    name: String,
    version: String,
    game: String,
    profile: String,
    fps: f32,
    frame_ms: f32,
    gpu_ms: f32,
    draws: u32,
    #[serde(default)]
    area: String,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    settings: String,
    #[serde(default)]
    hardware: String,
    /// The exact VICTORY_* settings of the run, so another user can try them.
    #[serde(default)]
    sets: BTreeMap<String, String>,
    /// The launcher that made the entry; `version` is the layer.
    #[serde(default)]
    launcher: String,
    when: u64,
    log: String,
    /// From the published board rather than this PC.
    #[serde(default)]
    remote: bool,
}

/// One fixed-width table cell, left-aligned, the same call in the header and rows.
fn table_cell(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, 20.0),
        egui::Layout::left_to_right(egui::Align::Center),
        add,
    );
}

fn table_sep(ui: &mut egui::Ui) {
    ui.add(egui::Separator::default().vertical().spacing(8.0));
}

fn small_label(text: &str) -> egui::RichText {
    egui::RichText::new(text)
        .small()
        .color(egui::Color32::from_rgb(140, 146, 158))
}

fn readable_value(key: &str, value: &str) -> String {
    if matches!(
        key,
        "VICTORY_VSYNC"
            | "VICTORY_TEXTURE_NO_WAIT"
            | "VICTORY_CARD_HASH_LANES"
            | "VICTORY_WIDE_BACK"
            | "VICTORY_JOIN_COPIES"
            | "VICTORY_INDIRECT_RUNS"
            | "VICTORY_INDIRECT_WINDOW"
            | "VICTORY_RELAY_QUEUE"
            | "VICTORY_BACK_OWN_VIEW"
            | "VICTORY_ALIAS_LAZY"
    ) {
        return match value {
            "1" => "on".into(),
            "0" => "off".into(),
            other => other.into(),
        };
    }
    if key.ends_with("_MB") {
        return format!("{value} MB");
    }
    if key.ends_with("_KB") {
        return format!("{value} KB");
    }
    value.to_string()
}

fn vram_label(mb: u32) -> String {
    if mb == 0 {
        "unknown VRAM".into()
    } else if mb >= 1024 {
        format!("{:.0} GB", mb as f32 / 1024.0)
    } else {
        format!("{mb} MB")
    }
}

fn settings_summary(settings: &Settings, hardware: &Hardware) -> String {
    let mut parts: Vec<String> = settings
        .sets
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect();
    parts.push(format!(
        "output={}",
        if settings.output.is_empty() {
            "native"
        } else {
            &settings.output
        }
    ));
    if settings.render_scale > 0.0 {
        parts.push(format!("render_scale={:.2}", settings.render_scale));
    }
    if settings.input_mode != "auto" {
        parts.push(format!("input={}", settings.input_mode));
    }
    if settings.mouse_look {
        parts.push("mouse_look=1".into());
        if !settings.mouse_buttons {
            parts.push("mouse_buttons=0".into());
        }
    }
    parts.push(format!("profile={}", settings.profile));
    format!(
        "{} | {} {}, {}",
        parts.join(" "),
        hardware.vendor.to_uppercase(),
        vram_label(hardware.vram_mb),
        std::env::consts::OS
    )
}

fn community_path() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join("community.json"))
}

fn known_game(game: &str) -> String {
    if game.starts_with("unknown") && game.contains("b99f876b") {
        "v1.0.2.1".to_string()
    } else {
        game.to_string()
    }
}

fn normalize_community(entries: &mut [CommunityEntry]) {
    for entry in entries {
        // Entries made by the launcher before it showed the layer version.
        if entry.launcher.is_empty() && entry.version.starts_with("0.0.") {
            entry.launcher = entry.version.clone();
            entry.version = LAYER_VERSION.to_string();
        }
        entry.game = known_game(&entry.game);
    }
}

fn load_community() -> Vec<CommunityEntry> {
    let mut entries: Vec<CommunityEntry> = community_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    normalize_community(&mut entries);
    entries
}

fn save_community(entries: &[CommunityEntry]) {
    let Some(path) = community_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let local: Vec<&CommunityEntry> = entries.iter().filter(|entry| !entry.remote).collect();
    if let Ok(text) = serde_json::to_string_pretty(&local) {
        let _ = std::fs::write(path, text);
    }
}

/// Where the published board lives (a file in the repository, served by
/// GitHub's raw host: no token, no server, and edits are visible in git).
const BOARD_URL: &str =
    "https://raw.githubusercontent.com/iNiKKo/Nullifier/main/community/leaderboard.json";

fn fetch_board() -> Option<Vec<CommunityEntry>> {
    let url = std::env::var("NULLIFIER_BOARD_URL").unwrap_or_else(|_| BOARD_URL.to_string());
    let out = if cfg!(windows) {
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "try {{ (Invoke-WebRequest -UseBasicParsing -Uri '{url}' -TimeoutSec 8).Content }} catch {{ }}"
                ),
            ])
            .output()
    } else {
        Command::new("curl")
            .args(["-fsSL", "--max-time", "8", &url])
            .output()
    }
    .ok()?;
    if !out.status.success() {
        return None;
    }
    let mut board: Vec<CommunityEntry> = serde_json::from_slice(&out.stdout).ok()?;
    // A published file is untrusted: keep the page responsive whatever it holds.
    board.retain(|entry| entry.name.len() <= 80 && entry.log.len() <= 200_000);
    board.truncate(500);
    Some(board)
}

fn date_of(epoch: u64) -> String {
    let days = (epoch / 86_400) as i64;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = year + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

fn mask_ipv4(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            let token = &text[start..i];
            let parts: Vec<&str> = token.split('.').collect();
            if parts.len() == 4 && parts.iter().all(|p| !p.is_empty() && p.parse::<u8>().is_ok()) {
                out.push_str("<ip>");
            } else {
                out.push_str(token);
            }
        } else {
            let ch = text[i..].chars().next().unwrap_or(' ');
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

fn mask_emails(line: &str) -> String {
    line.split_whitespace()
        .map(|word| {
            if word.contains('@') && word.contains('.') && !word.starts_with("http") {
                "<email>"
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The log as it is safe to share: paths, the machine's name and addresses
/// are replaced, and only the interesting lines are kept (header, frame
/// lines, failures, the resolution patch) so an entry is a few dozen KB.
fn sanitize_log(raw: &str, settings: &Settings) -> String {
    let clean = strip_ansi(raw);
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    let user = Path::new(&home)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_string();
    let prefix = data_dir()
        .map(|dir| dir.display().to_string())
        .unwrap_or_default();
    let game = settings.game_dir.trim().to_string();
    let mut kept: Vec<String> = Vec::new();
    for (index, line) in clean.lines().enumerate() {
        let keep = index < 25
            || line.contains(" fps,")
            || line.contains("device removed")
            || line.contains("unhandled exception")
            || line.contains("panicked")
            || line.contains(" ERROR ")
            || line.contains("resolution:")
            || line.contains("wineserver:")
            || line.contains("build ");
        if !keep {
            continue;
        }
        let mut line = line.trim_end().to_string();
        if !game.is_empty() {
            line = line.replace(&game, "<game>");
        }
        if !prefix.is_empty() {
            line = line.replace(&prefix, "<prefix>");
        }
        if !home.is_empty() {
            line = line.replace(&home, "<home>");
        }
        if !user.is_empty() {
            line = line.replace(&format!("/home/{user}"), "/home/<user>");
            line = line.replace(&format!(r"\Users\{user}"), r"\Users\<user>");
            line = line.replace(&format!("Users\\{user}"), "Users\\<user>");
        }
        line = mask_ipv4(&line);
        line = mask_emails(&line);
        kept.push(line);
    }
    if kept.len() > 240 {
        let mut cut = kept[..20].to_vec();
        cut.extend(kept[kept.len() - 220..].iter().cloned());
        cut.insert(20, format!("... {} lines omitted ...", kept.len() - cut.len()));
        cut.join("\n")
    } else {
        kept.join("\n")
    }
}

/// Median of the world frame lines (under 10 fps; menus are 30), with the
/// last frame time, GPU time and draw count.
fn world_stats(raw: &str) -> (Option<f32>, Option<f32>, Option<f32>, Option<u32>) {
    let clean = strip_ansi(raw);
    let mut values: Vec<f32> = Vec::new();
    let (mut frame, mut gpu, mut draws) = (None, None, None);
    for line in clean.lines() {
        let Some(rest) = after(line, "host: ") else { continue };
        let line_draws = before(rest, " draws/frame").and_then(last_num).map(|v| v as u32);
        if let Some(value) = before(rest, " fps,").and_then(|v| v.trim().parse().ok()) {
            if line_draws.is_some_and(|draws| draws >= 200) {
                values.push(value);
            }
        }
        if let Some(value) = before(rest, " ms/frame").and_then(last_num) {
            frame = Some(value);
        }
        if let Some(value) = before(rest, " on the GPU").and_then(last_num) {
            gpu = Some(value);
        }
        if let Some(value) = before(rest, " draws/frame").and_then(last_num) {
            draws = Some(value as u32);
        }
    }
    (median_fps(values), frame, gpu, draws)
}

fn game_label(raw: &str) -> String {
    let clean = strip_ansi(raw);
    for line in clean.lines().take(40) {
        if let Some(rest) = after(line, "build          ") {
            let label = known_game(rest.trim());
            return label.chars().take(40).collect();
        }
        if let Some(rest) = after(line, "build ") {
            if rest.contains('(') {
                let label = known_game(rest.trim());
                return label.chars().take(40).collect();
            }
        }
    }
    "unknown".into()
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

fn profile_hash(root: &Path) -> u64 {
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

fn data_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("nullifier"))
    } else {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
        Some(base.join("nullifier"))
    }
}

fn cache_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("nullifier"))
    } else {
        let base = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
        Some(base.join("nullifier"))
    }
}

// ---------------------------------------------------------------- hardware

#[derive(Clone)]
struct GameSetting {
    section: String,
    key: String,
    value: String,
    line: usize,
    indent: String,
}

struct GameSettings {
    path: PathBuf,
    lines: Vec<String>,
    settings: Vec<GameSetting>,
    dirty: bool,
    note: String,
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

fn load_game_settings() -> Option<GameSettings> {
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

fn section_title(section: &str) -> &'static str {
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

#[derive(Default, Clone)]
struct Hardware {
    vendor: String,
    vram_mb: u32,
    profile: String,
}

fn hardware(root: &Path) -> Hardware {
    let raw = detect(root);
    let mut hw = Hardware { ..Default::default() };
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
            hw.profile = rest
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_string();
        }
    }
    hw
}

fn detect(root: &Path) -> String {
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

// ---------------------------------------------------------------- profiles

struct Profile {
    name: String,
    sets: BTreeMap<String, String>,
    note: String,
    os: String,
    vendor: String,
    vram_min: u32,
    vram_max: u32,
    confidence: String,
}

fn profiles(root: &Path) -> Vec<Profile> {
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

fn os_name() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else {
        "linux"
    }
}

fn auto_profile(profiles: &[Profile], hw: &Hardware) -> String {
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

fn find_root() -> Option<PathBuf> {
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

// ---------------------------------------------------------------- running

fn launcher_path(root: &Path) -> PathBuf {
    if cfg!(windows) {
        root.join("launcher/nullifier-launch.ps1")
    } else {
        root.join("launcher/nullifier-launch.sh")
    }
}

fn settings_root_readiness(settings: &Settings) -> Readiness {
    settings.root.as_deref().map(readiness).unwrap_or_default()
}

/// The engine binary a run or a preload starts.
fn engine_exe(root: &Path) -> Option<PathBuf> {
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
fn readiness(root: &Path) -> Readiness {
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
fn pipeline_cache() -> Option<(usize, u64)> {
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
fn title_exe(dir: &Path) -> Option<PathBuf> {
    const WANT: &str = "game_scarlett_master.exe";
    let mut stack = vec![(dir.to_path_buf(), 0usize)];
    while let Some((at, depth)) = stack.pop() {
        if depth > 2 {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&at) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else { continue };
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

/// The stand-in pad's buttons, in the order the Controls page shows them.
const PAD_ACTIONS: [(&str, &str); 14] = [
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

fn load_keys(text: &str) -> Vec<String> {
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

fn keys_to_text(keys: &[String]) -> String {
    PAD_ACTIONS
        .iter()
        .zip(keys)
        .map(|((action, _), key)| format!("{}={}", key.trim(), action))
        .collect::<Vec<_>>()
        .join(",")
}

/// The key's name as the keyboard bindings spell it.
fn key_name(key: egui::Key) -> String {
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

fn same_key(bound: &str, key: &str) -> bool {
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

fn key_name_ok(name: &str) -> bool {
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
fn scan_controllers() -> Vec<String> {
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

fn custom_profile(settings: &Settings) -> Option<PathBuf> {
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

fn launch_command(root: &Path, settings: &Settings, profile_file: &Path) -> Command {
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
        cmd.env("VICTORY_RENDER_SCALE", format!("{:.2}", settings.render_scale));
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

fn check(root: Option<&Path>, settings: &Settings) {
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

// ---------------------------------------------------------------- the app

struct App {
    settings: Settings,
    profiles: Vec<Profile>,
    hardware: Hardware,
    tab: Tab,
    detect_output: String,
    child: Option<Child>,
    bench: Option<Child>,
    log_path: Option<PathBuf>,
    log_shown: String,
    log_refreshed: Option<std::time::Instant>,
    last_run_cache: LastRun,
    last_run_at: Option<std::time::Instant>,
    status: String,
    picking: bool,
    pick_dir: PathBuf,
    new_key: String,
    new_value: String,
    info: Option<String>,
    game: Option<GameSettings>,
    log_open: bool,
    bench_start: Option<std::time::Instant>,
    bench_world: Option<std::time::Instant>,
    bench_measure: Option<std::time::Instant>,
    bench_offset: usize,
    bench_phase: String,
    bench_progress: f32,
    bench_result: String,
    pick_for: Pick,
    readiness: Readiness,
    prep: Option<Child>,
    prep_kind: Prep,
    prep_log: Option<PathBuf>,
    prep_started: Option<std::time::Instant>,
    toast: Option<(String, std::time::Instant)>,
    game_advanced_open: bool,
    /// The graphics page asked before removing settings.xml.
    confirm_reset: bool,
    update_tag: Option<String>,
    update_rx: Option<std::sync::mpsc::Receiver<Option<String>>>,
    keyboard_keys: Vec<String>,
    controllers: Vec<String>,
    controllers_checked: bool,
    community: Vec<CommunityEntry>,
    community_open: Option<usize>,
    community_filter: String,
    community_game: String,
    community_area_filter: String,
    community_version_filter: String,
    community_sort: u8,
    community_notes_only: bool,
    community_status: String,
    community_rx: Option<std::sync::mpsc::Receiver<Option<Vec<CommunityEntry>>>>,
    community_fetching: bool,
    community_checked: bool,
    /// The last key seen by the input test.
    test_key: Option<String>,
    /// Mouse movement while the pointer is over the window.
    test_mouse: egui::Vec2,
    borrowed_from: Option<String>,
    borrowed_sets: Option<BTreeMap<String, String>>,
    borrowed_profile: Option<String>,
}

#[derive(Default, Clone, Copy)]
struct Readiness {
    engine: bool,
    wineserver: bool,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Prep {
    Engine,
    Wineserver,
    Preload,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Pick {
    Game,
    Capture,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum Preset {
    Recommended,
    LowMemory,
    Safe,
}

fn current_preset(sets: &BTreeMap<String, String>, shipped: &BTreeMap<String, String>) -> Option<Preset> {
    let safe = sets.get("VICTORY_LATE_WAIT").map(String::as_str) == Some("0")
        && !sets.contains_key("VICTORY_JOIN_COPIES")
        && !sets.contains_key("VICTORY_RELAY");
    if safe {
        return Some(Preset::Safe);
    }
    if sets.get("VICTORY_TEXTURE_BUDGET_MB").map(String::as_str) == Some("2048") {
        return Some(Preset::LowMemory);
    }
    if sets == shipped {
        return Some(Preset::Recommended);
    }
    None
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum Tab {
    Play,
    Graphics,
    Controls,
    Community,
    Rendering,
    Advanced,
    Debug,
}

#[derive(Clone, Default)]
struct LastRun {
    /// Median of the in-game frame lines (a few hundred draws or more).
    fps: Option<f32>,
    /// Median of menu/loading lines, kept to show what the menu runs at.
    menu_fps: Option<f32>,
    /// Whether any line was an in-game one (a few hundred draws or more).
    in_game: bool,
    frame: Option<f32>,
    copies_ms: Option<f32>,
    copies_mb: Option<f32>,
    textures_ms: Option<f32>,
    textures_mb: Option<f32>,
    gpu_ms: Option<f32>,
    back_ms: Option<f32>,
    heap_ms: Option<f32>,
    draws: Option<u32>,
    removals: u32,
}

fn median_fps(mut values: Vec<f32>) -> Option<f32> {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    match values.len() {
        0 => None,
        n if n % 2 == 1 => Some(values[n / 2]),
        n => Some((values[n / 2 - 1] + values[n / 2]) / 2.0),
    }
}

fn before<'a>(text: &'a str, marker: &str) -> Option<&'a str> {
    text.split_once(marker).map(|(left, _)| left)
}

fn after<'a>(text: &'a str, marker: &str) -> Option<&'a str> {
    text.split_once(marker).map(|(_, right)| right)
}

fn first_num(text: &str) -> Option<f32> {
    text.split_whitespace()
        .next()?
        .trim_end_matches(',')
        .parse()
        .ok()
}

fn last_num(text: &str) -> Option<f32> {
    let token = text.split_whitespace().last()?;
    token
        .trim_matches(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
        .parse()
        .ok()
}

fn megabytes(text: &str) -> Option<f32> {
    let first = text.split(';').next()?;
    let at = first.find(" MB")?;
    first[..at].split_whitespace().last()?.parse().ok()
}

fn frame_fps(text: &str) -> Vec<f32> {
    text.lines()
        .filter_map(|line| {
            let clean = strip_ansi(line);
            let rest = after(&clean, "host: ")?;
            let value = before(rest, " fps,")?;
            value.trim().parse::<f32>().ok()
        })
        .filter(|v| *v < 10.0)
        .collect()
}

fn median(values: &mut [f32]) -> f32 {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = values.len();
    if n == 0 {
        return 0.0;
    }
    if n.is_multiple_of(2) {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    } else {
        values[n / 2]
    }
}

fn percentile(values: &[f32], share: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let at = ((sorted.len() as f32 * share) as usize).max(1) - 1;
    sorted[at.min(sorted.len() - 1)]
}

impl App {
    fn new(settings: Settings) -> Self {
        let profiles = settings.root.as_deref().map(profiles).unwrap_or_default();
        let hardware = settings
            .root
            .as_deref()
            .map(hardware)
            .unwrap_or_default();
        let log_path = cache_dir()
            .map(|d| d.join("launcher-last.log"))
            .filter(|p| p.is_file());
        let log_shown = log_path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|t| tail(&t, 160))
            .unwrap_or_default();
        let readiness = settings_root_readiness(&settings);
        let (update_tag, update_rx) = update_check();
        let keyboard_keys = load_keys(&settings.keyboard);
        let status = if settings.reloaded_profile {
            "profile updated; settings reloaded"
        } else {
            "ready"
        };
        Self {
            settings,
            profiles,
            hardware,
            tab: Tab::Play,
            detect_output: String::new(),
            child: None,
            bench: None,
            log_path,
            log_shown,
            log_refreshed: None,
            last_run_cache: LastRun::default(),
            last_run_at: None,
            status: status.into(),
            picking: false,
            pick_dir: home_dir().unwrap_or_else(|| PathBuf::from(".")),
            new_key: String::new(),
            new_value: String::new(),
            info: None,
            game: load_game_settings(),
            log_open: false,
            bench_start: None,
            bench_world: None,
            bench_measure: None,
            bench_offset: 0,
            bench_phase: String::new(),
            bench_progress: 0.0,
            bench_result: String::new(),
            pick_for: Pick::Game,
            readiness,
            prep: None,
            prep_kind: Prep::Engine,
            prep_log: None,
            prep_started: None,
            toast: None,
            game_advanced_open: false,
            confirm_reset: false,
            update_tag,
            update_rx,
            keyboard_keys,
            controllers: Vec::new(),
            controllers_checked: false,
            community: load_community(),
            community_open: None,
            community_filter: String::new(),
            community_game: String::new(),
            community_area_filter: String::new(),
            community_version_filter: String::new(),
            community_sort: 0,
            community_notes_only: false,
            community_status: String::new(),
            community_rx: None,
            community_fetching: false,
            community_checked: false,
            test_key: None,
            test_mouse: egui::Vec2::ZERO,
            borrowed_from: None,
            borrowed_sets: None,
            borrowed_profile: None,
        }
    }

    fn running(&mut self) -> bool {
        match self.child.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => false,
        }
    }

    fn bench_running(&mut self) -> bool {
        match self.bench.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => false,
        }
    }

    fn launch(&mut self) {
        let Some(root) = self.settings.root.clone() else {
            self.status = "repo root not found".into();
            return;
        };
        if self.settings.game_dir.trim().is_empty() {
            self.status = "choose the game folder first".into();
            return;
        }
        let Some(profile) = custom_profile(&self.settings) else {
            self.status = "cannot write the temporary profile".into();
            return;
        };
        let log = cache_dir().map(|d| d.join("launcher-last.log"));
        if let Some(path) = &log {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
        }
        let mut cmd = launch_command(&root, &self.settings, &profile);
        if let Some(path) = &log {
            if let Ok(file) = std::fs::File::create(path) {
                if let Ok(clone) = file.try_clone() {
                    cmd.stdout(Stdio::from(clone));
                }
                cmd.stderr(Stdio::from(file));
            }
        }
        self.settings.save();
        match cmd.spawn() {
            Ok(child) => {
                self.child = Some(child);
                self.log_path = log;
                self.status = "running".into();
            }
            Err(e) => self.status = format!("cannot launch: {e}"),
        }
    }

    fn bench_now(&mut self) {
        if self.running() {
            self.status = "close the game before measuring".into();
            return;
        }
        let Some(root) = self.settings.root.clone() else { return };
        let Some(profile) = custom_profile(&self.settings) else { return };
        let log = cache_dir().map(|d| d.join("bench-last.log"));
        // No screenshots or scripted keys during a measurement: they would
        // change the number it reports.
        let mut quiet = self.settings.clone();
        quiet.capture_dir.clear();
        quiet.keys.clear();
        let mut cmd = launch_command(&root, &quiet, &profile);
        if let Some(path) = &log {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(file) = std::fs::File::create(path) {
                if let Ok(clone) = file.try_clone() {
                    cmd.stdout(Stdio::from(clone));
                }
                cmd.stderr(Stdio::from(file));
            }
        }
        match cmd.spawn() {
            Ok(child) => {
                self.bench = Some(child);
                self.log_path = log;
                self.bench_start = Some(std::time::Instant::now());
                self.bench_world = None;
                self.bench_measure = None;
                self.bench_offset = 0;
                self.bench_progress = 0.02;
                self.bench_phase = "Starting".into();
                self.bench_result.clear();
                self.status = "benchmark running".into();
            }
            Err(e) => self.status = format!("cannot benchmark: {e}"),
        }
    }

    fn prep_running(&mut self) -> bool {
        match self.prep.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => false,
        }
    }

    fn prepare(&mut self, kind: Prep) {
        if self.prep_running() {
            return;
        }
        let Some(root) = self.settings.root.clone() else {
            self.status = "repo root not found".into();
            return;
        };
        if cfg!(windows) && kind == Prep::Wineserver {
            self.status = "the built wineserver is not used on Windows".into();
            return;
        }
        let script = match kind {
            Prep::Engine => root.join("tools/setup.sh"),
            Prep::Wineserver => root.join("tools/build-wineserver.sh"),
            Prep::Preload => root.join("tools/preload.sh"),
        };
        if !cfg!(windows) && !script.is_file() {
            self.status = format!("missing {}", script.display());
            return;
        }
        let name = match kind {
            Prep::Engine => "setup.log",
            Prep::Wineserver => "wineserver.log",
            Prep::Preload => "preload.log",
        };
        let log = cache_dir().map(|d| d.join(name));
        if let Some(path) = &log {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
        }
        let mut cmd = if cfg!(windows) {
            // The scripts are bash; Windows has cargo and the engine itself.
            match kind {
                Prep::Engine => {
                    let mut c = Command::new("cargo");
                    c.args(["build", "--release", "-p", "nullifier"])
                        .current_dir(&root);
                    c
                }
                Prep::Preload => {
                    let Some(exe) = engine_exe(&root) else {
                        self.status = "the engine is not built yet - build it first".into();
                        return;
                    };
                    let mut c = Command::new(exe);
                    c.arg("warm")
                        .arg(self.settings.game_dir.trim())
                        .current_dir(&root);
                    c
                }
                Prep::Wineserver => unreachable!(),
            }
        } else {
            let mut c = Command::new("sh");
            c.arg(&script).current_dir(&root);
            if kind == Prep::Preload {
                c.env("VICTORY_GAME_DIR", &self.settings.game_dir);
                if let Some(profile) = custom_profile(&self.settings) {
                    c.env("VICTORY_PROFILES", profile);
                }
                if !self.settings.keyboard.trim().is_empty() {
                    c.env("VICTORY_KEYBOARD", self.settings.keyboard.trim());
                }
            }
            c
        };
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        if let Some(path) = &log {
            if let Ok(file) = std::fs::File::create(path) {
                if let Ok(clone) = file.try_clone() {
                    cmd.stdout(Stdio::from(clone));
                }
                cmd.stderr(Stdio::from(file));
            }
        }
        let label = if cfg!(windows) {
            match kind {
                Prep::Engine => "cargo".to_string(),
                Prep::Preload => "the engine".to_string(),
                Prep::Wineserver => String::new(),
            }
        } else {
            script.display().to_string()
        };
        match cmd.spawn() {
            Ok(child) => {
                self.prep = Some(child);
                self.prep_kind = kind;
                self.prep_log = log;
                self.prep_started = Some(std::time::Instant::now());
                self.log_path = self.prep_log.clone();
                self.status = match kind {
                    Prep::Engine => "setting up the engine".into(),
                    Prep::Wineserver => "building the fast graphics service".into(),
                    Prep::Preload => "preloading shaders".into(),
                };
            }
            Err(e) => self.status = format!("cannot run {label}: {e}"),
        }
    }

    fn tick_prepare(&mut self, ctx: &egui::Context) {
        let Some(child) = self.prep.as_mut() else { return };
        match child.try_wait() {
            Ok(Some(status)) => {
                self.prep = None;
                let kind = self.prep_kind;
                self.readiness = settings_root_readiness(&self.settings);
                let ok = status.success();
                self.status = match (kind, ok) {
                    (Prep::Engine, true) => "the engine is ready".into(),
                    (Prep::Engine, false) => "setup failed - the details are in the log".into(),
                    (Prep::Wineserver, true) => "the fast graphics service is ready".into(),
                    (Prep::Wineserver, false) => "the fast graphics service did not build - the log has the details".into(),
                    (Prep::Preload, true) => "shaders preloaded".into(),
                    (Prep::Preload, false) => "preloading stopped early - the log has the details".into(),
                };
                self.toast = Some((self.status.clone(), std::time::Instant::now()));
                if !ok {
                    self.log_open = true;
                }
            }
            Ok(None) => ctx.request_repaint_after(Duration::from_millis(500)),
            Err(_) => {
                self.prep = None;
                self.status = "setup stopped".into();
            }
        }
    }

    fn stop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
        }
        if let Some(child) = self.bench.as_mut() {
            let _ = child.kill();
        }
        if let Some(child) = self.prep.as_mut() {
            let _ = child.kill();
        }
        self.child = None;
        self.bench = None;
        self.prep = None;
        kill_game();
        self.bench_start = None;
        self.bench_world = None;
        self.bench_measure = None;
        self.status = "stopped".into();
    }

    fn tick_bench(&mut self) {
        let Some(start) = self.bench_start else { return };
        let elapsed = start.elapsed().as_secs_f32();
        let text = self
            .log_path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        let seen = frame_fps(&text);
        if self.bench_world.is_none() {
            if seen.len() >= 2 && seen[seen.len() - 1] < 10.0 && seen[seen.len() - 2] < 10.0 {
                self.bench_world = Some(std::time::Instant::now());
                self.bench_phase = "Warming up".into();
                self.bench_progress = 0.5;
            } else {
                self.bench_progress = (0.02 + elapsed / 150.0 * 0.45).min(0.45);
                self.bench_phase = format!("Loading the game ({elapsed:.0} s)");
            }
        }
        if let Some(world) = self.bench_world {
            if self.bench_measure.is_none() {
                if world.elapsed().as_secs_f32() >= 6.0 {
                    self.bench_measure = Some(std::time::Instant::now());
                    self.bench_offset = text.len();
                    self.bench_phase = "Measuring the frame rate".into();
                    self.bench_progress = 0.65;
                } else {
                    self.bench_progress = 0.5 + 0.15 * (world.elapsed().as_secs_f32() / 6.0);
                    self.bench_phase = "Warming up".into();
                }
            }
        }
        if let Some(measure) = self.bench_measure {
            let into = measure.elapsed().as_secs_f32();
            if into >= 20.0 {
                let from = self.bench_offset.min(text.len());
                let slice = &text[from..];
                let mut values = frame_fps(slice);
                if values.is_empty() {
                    values = seen.clone();
                }
                let mut sorted = values.clone();
                let med = median(&mut sorted);
                let p10 = percentile(&values, 0.1);
                let crashes = slice.matches("device removed").count();
                let outcome = if crashes == 0 {
                    "no crashes".to_string()
                } else {
                    format!("{crashes} crashes")
                };
                self.bench_result = format!(
                    "{med:.2} fps · p10 {p10:.2} · {} frames · {outcome}",
                    values.len()
                );
                if let Some(child) = self.bench.as_mut() {
                    let _ = child.kill();
                }
                self.bench = None;
                kill_game();
                self.bench_start = None;
                self.bench_world = None;
                self.bench_measure = None;
                self.bench_progress = 1.0;
                self.bench_phase = "Done".into();
                self.status = "benchmark done".into();
            } else {
                self.bench_progress = 0.65 + 0.3 * (into / 20.0);
                self.bench_phase = "Measuring the frame rate".into();
            }
        }
    }

    fn refresh_log(&mut self) {
        // A 6 MB log read and stripped every frame made the window crawl.
        // Twice a second at most, and only the tail of the file.
        if let Some(at) = self.log_refreshed {
            if at.elapsed() < Duration::from_millis(500) {
                return;
            }
        }
        self.log_refreshed = Some(std::time::Instant::now());
        let Some(path) = &self.log_path else { return };
        let Some(text) = read_tail(path, 96 * 1024) else {
            return;
        };
        let clean: String = text.lines().map(strip_ansi).collect::<Vec<_>>().join("\n");
        self.log_shown = tail(&clean, 160);
    }

    fn last_run(&mut self) -> LastRun {
        // Parsing the whole log every frame is also expensive; a change of the
        // file's bytes is the reason to do it, checked at most every two
        // seconds.
        if let Some(at) = self.last_run_at {
            if at.elapsed() < Duration::from_secs(2) {
                return self.last_run_cache.clone();
            }
        }
        let run = self.read_last_run();
        self.last_run_cache = run.clone();
        self.last_run_at = Some(std::time::Instant::now());
        run
    }

    fn read_last_run(&self) -> LastRun {
        let mut run = LastRun::default();
        let Some(path) = &self.log_path else { return run };
        let Some(text) = read_tail(path, 2 * 1024 * 1024) else { return run };
        run.removals = text.matches("device removed").count() as u32;
        let mut world: Vec<f32> = Vec::new();
        let mut menus: Vec<f32> = Vec::new();
        let mut last_fps = None;
        for line in text.lines() {
            let clean = strip_ansi(line);
            let Some(rest) = after(&clean, "host: ") else { continue };
            let mut line_draws = None;
            if let Some(v) = before(rest, " draws/frame").and_then(last_num) {
                line_draws = Some(v as u32);
            }
            if let Some(v) = before(rest, " fps,").and_then(|v| v.trim().parse::<f32>().ok()) {
                last_fps = Some(v);
                if line_draws.is_some_and(|draws| draws >= 200) {
                    world.push(v);
                } else {
                    menus.push(v);
                }
            }
            if let Some(v) = before(rest, " ms/frame") {
                run.frame = last_num(v);
            }
            if let Some(part) = after(rest, "copies ") {
                if let Some(v) = first_num(part) {
                    run.copies_ms = Some(v);
                }
                if let Some(v) = megabytes(part) {
                    run.copies_mb = Some(v);
                }
            }
            if let Some(part) = after(rest, "textures ") {
                if let Some(v) = first_num(part) {
                    run.textures_ms = Some(v);
                }
                if let Some(v) = megabytes(part) {
                    run.textures_mb = Some(v);
                }
            }
            if let Some(v) = before(rest, " on the GPU").and_then(last_num) {
                run.gpu_ms = Some(v);
            }
            if let Some(v) = before(rest, " copying back").and_then(last_num) {
                run.back_ms = Some(v);
            }
            if let Some(part) = after(rest, "passes ") {
                if let Some(v) = before(part, " ms").and_then(|v| v.trim().parse().ok()) {
                    run.heap_ms = Some(v);
                }
            }
            if let Some(draws) = line_draws {
                run.draws = Some(draws);
            }
        }
        run.in_game = !world.is_empty();
        run.fps = median_fps(world).or(last_fps);
        run.menu_fps = median_fps(menus).or_else(|| {
            // A menu log with no draws recorded still has a 30 fps line.
            last_fps.filter(|fps| *fps >= 15.0)
        });
        run
    }

    fn load_profile(&mut self, name: &str) {
        if let Some(profile) = self.profiles.iter().find(|p| p.name == name) {
            self.settings.profile = name.to_string();
            self.settings.sets = profile.sets.clone();
            if let Some(root) = &self.settings.root {
                self.settings.profiles_hash = profile_hash(root);
            }
            self.status = format!("loaded {}", name);
        }
    }

    fn detect_now(&mut self) {
        if let Some(root) = &self.settings.root {
            self.detect_output = detect(root);
            self.hardware = hardware(root);
        }
    }

    fn profile(&self) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.name == self.settings.profile)
    }

    fn play(&mut self, ui: &mut egui::Ui) {
        page_header(ui, "Play", "");
        let warn = warnings(&self.settings, &self.hardware);
        warnings_card(ui, &warn);
        if let Some(name) = self.borrowed_from.clone() {
            ui.add_space(4.0);
            egui::Frame::group(ui.style())
                .fill(egui::Color32::from_rgb(45, 38, 24))
                .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(120, 95, 45)))
                .inner_margin(egui::Margin::symmetric(14, 10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "Using {name}'s settings — temporary"
                            ))
                            .color(egui::Color32::from_rgb(240, 200, 120)),
                        );
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if ui.add(secondary("Revert", 80.0, 28.0)).clicked() {
                                    self.revert_settings();
                                }
                                if ui.add(primary("Keep", 70.0, 28.0)).clicked() {
                                    self.keep_settings();
                                }
                            },
                        );
                    });
                });
        }
        if let Some((text, at)) = &self.toast {
            if at.elapsed() < Duration::from_secs(6) {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(text)
                        .small()
                        .color(egui::Color32::from_rgb(87, 199, 133)),
                );
            }
        }
        ui.add_space(4.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            let dir = PathBuf::from(self.settings.game_dir.trim());
            let found = dir.is_dir().then(|| title_exe(&dir)).flatten();
            if self.settings.game_dir.trim().is_empty() {
                self.welcome(ui);
            } else if !dir.is_dir() || found.is_none() {
                self.not_a_game(ui, &dir);
            } else {
                if !self.readiness.engine {
                    self.engine_setup(ui);
                } else {
                    self.launch_card(ui, &dir, found.as_deref());
                }
                if self.prep.is_some() {
                    self.progress_card(ui);
                } else if self.readiness.engine && !self.readiness.wineserver {
                    self.wineserver_tip(ui);
                }
                ui.add_space(6.0);
                if self.readiness.engine {
                    ui.columns(2, |cols| {
                        self.quick(&mut cols[0]);
                        self.last_run_card(&mut cols[1]);
                    });
                }
            }
        });
    }

    fn open_game_picker(&mut self) {
        self.pick_for = Pick::Game;
        self.pick_dir = PathBuf::from(&self.settings.game_dir);
        if !self.pick_dir.is_dir() {
            self.pick_dir = home_dir().unwrap_or_else(|| PathBuf::from("/"));
        }
        self.picking = true;
    }

    fn welcome(&mut self, ui: &mut egui::Ui) {
        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::symmetric(20, 20))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new("Welcome to Nullifier").size(21.0).strong());
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(
                            "Play your Xbox copy on this PC. Nothing is uploaded and no account is needed: the game stays in the folder you choose.",
                        )
                        .color(egui::Color32::from_rgb(178, 183, 194)),
                    )
                    .wrap(),
                );
                ui.add_space(12.0);
                egui::Frame::group(ui.style())
                    .fill(egui::Color32::from_rgb(27, 30, 36))
                    .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(64, 70, 82)))
                    .inner_margin(egui::Margin::symmetric(16, 22))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.vertical_centered(|ui| {
                            ui.label(egui::RichText::new("Drop your game folder here").size(15.0).strong());
                            ui.label(
                                egui::RichText::new("or choose it below")
                                    .small()
                                    .color(egui::Color32::GRAY),
                            );
                        });
                    });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.add(primary("Choose game folder…", 190.0, 36.0)).clicked() {
                        self.open_game_picker();
                    }
                    ui.label(
                        egui::RichText::new("The folder with game_scarlett_master.exe")
                            .small()
                            .color(egui::Color32::GRAY),
                    );
                });
            });
    }

    fn not_a_game(&mut self, ui: &mut egui::Ui, dir: &Path) {
        card(ui, |ui| {
            ui.label(
                egui::RichText::new("That folder doesn't look like the game")
                    .size(16.0)
                    .strong(),
            );
            ui.add(
                egui::Label::new(
                    egui::RichText::new(if dir.is_dir() {
                        "game_scarlett_master.exe was not found in it, or in the two folders below it."
                    } else {
                        "The folder no longer exists. It may have been moved or renamed."
                    })
                    .color(egui::Color32::from_rgb(232, 179, 74)),
                )
                .wrap(),
            );
            ui.add_space(4.0);
            ui.add(
                egui::Label::new(
                    egui::RichText::new(dir.display().to_string())
                        .small()
                        .color(egui::Color32::GRAY),
                )
                .wrap(),
            );
            ui.add_space(8.0);
            if ui.add(primary("Choose the right folder…", 200.0, 34.0)).clicked() {
                self.open_game_picker();
            }
            ui.label(
                egui::RichText::new("It is usually the folder that holds the game's .exe files.")
                    .small()
                    .color(egui::Color32::GRAY),
            );
        });
    }

    fn engine_setup(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.label(egui::RichText::new("One-time setup").size(16.0).strong());
            ui.add(
                egui::Label::new(
                    egui::RichText::new(
                        "Nullifier needs to build its engine for this PC. It takes about ten minutes the first time; later starts are instant.",
                    )
                    .color(egui::Color32::from_rgb(178, 183, 194)),
                )
                .wrap(),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let busy = self.prep.is_some();
                if ui
                    .add_enabled(!busy, primary("Set up now", 170.0, 34.0))
                    .clicked()
                {
                    self.prepare(Prep::Engine);
                }
                ui.label(
                    egui::RichText::new(if cfg!(windows) {
                        "Needs the Rust toolchain with MSVC, and an internet connection."
                    } else {
                        "Needs clang and lld, and an internet connection."
                    })
                    .small()
                    .color(egui::Color32::GRAY),
                );
            });
        });
    }

    fn launch_card(&mut self, ui: &mut egui::Ui, dir: &Path, _exe: Option<&Path>) {
        card(ui, |ui| {
            let vendor = if self.hardware.vendor.is_empty() {
                "your hardware".to_string()
            } else {
                format!(
                    "{} · {}",
                    self.hardware.vendor.to_uppercase(),
                    vram_label(self.hardware.vram_mb)
                )
            };
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    let name = dir
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("game");
                    ui.add(
                        egui::Label::new(egui::RichText::new(name).size(18.0).strong())
                            .truncate(),
                    )
                    .on_hover_text(dir.display().to_string());
                    let state = if self.running() {
                        egui::RichText::new("running · Esc pauses")
                            .small()
                            .color(egui::Color32::from_rgb(87, 199, 133))
                    } else {
                        egui::RichText::new(format!("game found · {vendor}"))
                            .small()
                            .color(egui::Color32::from_rgb(87, 199, 133))
                    };
                    ui.label(state);
                    if !self.settings.output.is_empty() {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} output · experimental",
                                self.settings.output
                            ))
                            .small()
                            .color(egui::Color32::from_rgb(232, 179, 74)),
                        );
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let button = if self.running() {
                        egui::Button::new(
                            egui::RichText::new("Stop").strong().color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(176, 68, 68))
                        .min_size(egui::vec2(150.0, 40.0))
                    } else {
                        primary("Play", 150.0, 40.0)
                    };
                    if ui.add_enabled(!self.prep.is_some() || self.running(), button).clicked() {
                        if self.running() {
                            self.stop();
                        } else {
                            self.launch();
                        }
                    }
                    ui.allocate_ui_with_layout(
                        egui::vec2(52.0, 40.0),
                        egui::Layout::centered_and_justified(egui::Direction::TopDown),
                        |ui| {
                            egui::menu::menu_custom_button(
                                ui,
                                egui::Button::new(
                                    egui::RichText::new("•  •  •").size(12.0),
                                )
                                .min_size(egui::vec2(48.0, 32.0)),
                                |ui| {
                                if ui.button("Change folder…").clicked() {
                                    self.open_game_picker();
                                    ui.close_menu();
                                }
                                if ui.button("Settings…").clicked() {
                                    self.settings.show_advanced = true;
                                    self.tab = Tab::Rendering;
                                    self.settings.save();
                                    ui.close_menu();
                                }
                                if ui.button("Open game folder").clicked() {
                                    open_path(dir);
                                    ui.close_menu();
                                }
                                },
                            );
                        },
                    );
                });
            });
            ui.separator();
            let cold = pipeline_cache().map(|(count, _)| count == 0).unwrap_or(true);
            ui.horizontal(|ui| {
                match pipeline_cache() {
                    Some((count, bytes)) if count > 0 => {
                        ui.label(
                            egui::RichText::new("Shader cache")
                                .small()
                                .color(egui::Color32::from_rgb(150, 156, 168)),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "{count} pipelines · {:.0} KB",
                                bytes as f32 / 1024.0
                            ))
                            .small(),
                        );
                    }
                    _ => {
                        ui.label(
                            egui::RichText::new("Shader cache is empty — the first run compiles while it loads.")
                                .small()
                                .color(egui::Color32::from_rgb(232, 179, 74)),
                        );
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let can = self.prep.is_none() && !self.running();
                    let label = if cold { "Preload first" } else { "Preload" };
                    if ui
                        .add_enabled(can, secondary(label, 110.0, 28.0))
                        .on_hover_text("Run the opening once to compile and save the pipelines.")
                        .clicked()
                    {
                        self.prepare(Prep::Preload);
                    }
                });
            });
        });
    }

    fn wineserver_tip(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        egui::Frame::group(ui.style())
            .fill(egui::Color32::from_rgb(45, 38, 24))
            .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(120, 95, 45)))
            .inner_margin(egui::Margin::symmetric(14, 12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    egui::RichText::new("Make it several times faster (one time)")
                        .strong()
                        .color(egui::Color32::from_rgb(240, 200, 120)),
                );
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(
                            "Nullifier can build its own graphics service. It is the single biggest speed-up.",
                        )
                        .color(egui::Color32::from_rgb(225, 210, 180)),
                    )
                    .wrap(),
                );
                ui.add_space(6.0);
                if ui
                    .add_enabled(self.prep.is_none(), primary("Build it now", 150.0, 32.0))
                    .clicked()
                {
                    self.prepare(Prep::Wineserver);
                }
            });
    }

    fn progress_card(&mut self, ui: &mut egui::Ui) {
        let title = match self.prep_kind {
            Prep::Engine => "Setting up the engine",
            Prep::Wineserver => "Building the fast graphics service",
            Prep::Preload => "Preloading shaders",
        };
        let elapsed = self
            .prep_started
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0);
        let tail_text = self
            .prep_log
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|t| tail(&t, 8))
            .unwrap_or_default();
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(egui::RichText::new(title).size(16.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if chip_button(ui, "Cancel").clicked() {
                        self.stop();
                    }
                });
            });
            ui.label(
                egui::RichText::new(format!(
                    "{}:{:02} elapsed. This runs once; later starts are instant.",
                    elapsed / 60,
                    elapsed % 60
                ))
                .small()
                .color(egui::Color32::GRAY),
            );
            if !tail_text.trim().is_empty() {
                ui.add_space(6.0);
                egui::Frame::group(ui.style())
                    .fill(egui::Color32::from_rgb(20, 22, 26))
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(tail_text.trim_end())
                                    .monospace()
                                    .small()
                                    .color(egui::Color32::from_rgb(150, 156, 168)),
                            )
                            .wrap(),
                        );
                    });
            }
        });
    }

    fn runtime_card(&mut self, ui: &mut egui::Ui) {
        let cache = cache_dir().unwrap_or_default();
        card(ui, |ui| {
            ui.strong("Installation");
            if cfg!(windows) {
                ui.label("Runs natively on Windows; no Wine or prefix is used.");
            } else {
                let base = data_dir().unwrap_or_default();
                let prefix = base.join("prefix");
                let tree = base.join("wineserver/bin/wineserver").is_file();
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Graphics service").small());
                    ui.label(
                        egui::RichText::new(if tree { "optimised" } else { "standard" })
                            .small()
                            .color(if tree { egui::Color32::from_rgb(87, 199, 133) } else { egui::Color32::from_rgb(232, 179, 74) }),
                    );
                    ui.add_space(12.0);
                    ui.label(egui::RichText::new("Game data").small());
                    ui.label(egui::RichText::new(prefix.display().to_string()).small().color(egui::Color32::GRAY));
                });
                ui.horizontal(|ui| {
                    if chip_button(ui, "Open game data").clicked() {
                        open_path(&prefix);
                    }
                    if chip_button(ui, "Open cache").clicked() {
                        open_path(&cache);
                    }
                    if chip_button(ui, "Open log").clicked() {
                        if let Some(path) = &self.log_path {
                            open_path(path);
                        }
                    }
                });
            }
        });
    }

    fn quick(&mut self, ui: &mut egui::Ui) {
        let auto = auto_profile(&self.profiles, &self.hardware);
        let shipped = self
            .profiles
            .iter()
            .find(|p| p.name == auto)
            .map(|p| p.sets.clone())
            .unwrap_or_default();
        let current = current_preset(&self.settings.sets, &shipped);
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Performance");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .link(egui::RichText::new("Customise…").small())
                        .clicked()
                    {
                        self.settings.show_advanced = true;
                        self.tab = Tab::Rendering;
                        self.settings.save();
                    }
                });
            });
            ui.add(
                egui::Label::new(
                    egui::RichText::new("Choose a preset; the numbers are in Rendering.")
                        .small()
                        .color(egui::Color32::GRAY),
                ),
            );
            ui.add_space(2.0);
            let mut chosen = None;
            for (preset, title, desc) in [
                (Preset::Recommended, "Recommended", "Measured for your graphics card."),
                (Preset::LowMemory, "Low memory", "For an 8 GB card or a busy desktop."),
                (Preset::Safe, "Safe", "If it crashes or loses the device."),
            ] {
                if preset_card(ui, current == Some(preset), title, desc).clicked() {
                    chosen = Some(preset);
                }
            }
            if let Some(preset) = chosen {
                let mut sets = shipped.clone();
                match preset {
                    Preset::Recommended => {}
                    Preset::LowMemory => {
                        sets.insert("VICTORY_TEXTURE_BUDGET_MB".into(), "2048".into());
                        sets.insert("VICTORY_CARD_BUFFERS_MB".into(), "256".into());
                    }
                    Preset::Safe => {
                        sets.insert("VICTORY_LATE_WAIT".into(), "0".into());
                        sets.remove("VICTORY_JOIN_COPIES");
                        sets.remove("VICTORY_RELAY");
                    }
                }
                self.settings.profile = auto.clone();
                self.settings.sets = sets;
                self.status = format!("{preset:?} preset");
            }
        });
    }

    fn last_run_card(&mut self, ui: &mut egui::Ui) {
        let run = self.last_run();
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Last run");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !self.running() && !self.bench_running(),
                            secondary("Share…", 90.0, 30.0),
                        )
                        .on_hover_text("Open Community with this run ready to share.")
                        .clicked()
                    {
                        self.tab = Tab::Community;
                    }
                });
            });
            if run.fps.is_none() {
                ui.label(egui::RichText::new("No run yet.").color(egui::Color32::GRAY));
                return;
            }
            ui.horizontal(|ui| {
                ui.heading(format!("{:.1}", run.fps.unwrap_or(0.0)));
                ui.label(if run.in_game { "fps in game" } else { "fps (menu/loading)" });
            });
            ui.horizontal_wrapped(|ui| {
                if run.in_game {
                    if let Some(menu) = run.menu_fps {
                        ui.label(
                            egui::RichText::new(format!("menu {menu:.0} fps"))
                                .small()
                                .color(egui::Color32::GRAY),
                        );
                    }
                }
                if let Some(draws) = run.draws {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new(format!("{draws} draws a frame")).small().color(egui::Color32::GRAY));
                }
                ui.add_space(8.0);
                if run.removals == 0 {
                    ui.label(
                        egui::RichText::new("no crashes")
                            .small()
                            .color(egui::Color32::from_rgb(87, 199, 133)),
                    );
                } else {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} crash{}",
                            run.removals,
                            if run.removals == 1 { "" } else { "es" }
                        ))
                        .small()
                        .color(egui::Color32::from_rgb(232, 106, 106)),
                    );
                }
            });
            egui::Grid::new("last run")
                .num_columns(2)
                .spacing([18.0, 6.0])
                .show(ui, |ui| {
                    stat(ui, "Frame", run.frame.map(|v| format!("{v:.1} ms")));
                    stat(ui, "GPU", run.gpu_ms.map(|v| format!("{v:.1} ms")));
                    ui.end_row();
                    stat(ui, "Copies", pair(run.copies_ms, run.copies_mb));
                    stat(ui, "Textures", pair(run.textures_ms, run.textures_mb));
                    ui.end_row();
                    stat(ui, "Copy back", run.back_ms.map(|v| format!("{v:.1} ms")));
                    stat(ui, "Card check", run.heap_ms.map(|v| format!("{v:.1} ms")));
                    ui.end_row();
                });
        });
    }

    fn game_page(&mut self, ui: &mut egui::Ui) {
        page_header(
            ui,
            "Graphics",
            "The game's own graphics settings, from its settings.xml. Close the game before changing them.",
        );
        egui::Frame::group(ui.style())
            .fill(egui::Color32::from_rgb(45, 38, 24))
            .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(120, 95, 45)))
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    egui::RichText::new(
                        "Experimental — some settings are saved but do not affect the game yet.",
                    )
                    .color(egui::Color32::from_rgb(240, 200, 120)),
                );
            });
        let running = self.running();
        let mut reload = false;
        let mut advanced_open = self.game_advanced_open;
        let Some(game) = self.game.as_mut() else {
            card(ui, |ui| {
                ui.label("Not found yet. Start the game once so it writes its settings, then press Refresh.");
                if ui.button("Refresh").clicked() {
                    reload = true;
                }
            });
            if reload {
                self.game = load_game_settings();
            }
            return;
        };
        let planned = &mut self.settings.planned;
        let mut reset = false;
        let mut confirm_reset = self.confirm_reset;
        let backup = game.path.with_file_name("settings.xml.bak");
        let has_backup = backup.exists();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!running, primary("Apply", 80.0, 30.0))
                .on_hover_text("Write settings.xml, keeping settings.xml.bak")
                .clicked()
            {
                match apply_game(game) {
                    Ok(()) => {
                        game.note = "Saved (backup in settings.xml.bak)".into();
                        game.dirty = false;
                    }
                    Err(e) => game.note = format!("Could not save: {e}"),
                }
            }
            if ui
                .add_enabled(!running, secondary("Discard edits", 110.0, 30.0))
                .on_hover_text("Read settings.xml again and throw away what was not applied.")
                .clicked()
            {
                reload = true;
            }
            if ui
                .add_enabled(
                    !running && has_backup,
                    secondary("Restore backup", 120.0, 30.0),
                )
                .on_hover_text(if has_backup {
                    "Put settings.xml.bak back: the state before the last Apply."
                } else {
                    "No backup yet. Apply once to create settings.xml.bak."
                })
                .clicked()
            {
                if std::fs::copy(&backup, &game.path).is_ok() {
                    game.note = "Backup restored".into();
                    reload = true;
                } else {
                    game.note = "Could not restore settings.xml.bak".into();
                }
            }
            if confirm_reset {
                if ui
                    .add_enabled(!running, primary("Yes, reset", 96.0, 30.0))
                    .clicked()
                {
                    let aside = game.path.with_file_name("settings.xml.reset");
                    match std::fs::copy(&game.path, &aside)
                        .and_then(|_| std::fs::remove_file(&game.path))
                    {
                        Ok(()) => reset = true,
                        Err(e) => game.note = format!("Could not reset: {e}"),
                    }
                    confirm_reset = false;
                }
                if ui
                    .add(secondary("Cancel", 70.0, 30.0))
                    .clicked()
                {
                    confirm_reset = false;
                }
            } else if ui
                .add_enabled(!running, secondary("Reset to defaults", 140.0, 30.0))
                .on_hover_text(
                    "Move settings.xml aside (settings.xml.reset keeps a copy). The game writes fresh defaults the next time it starts.",
                )
                .clicked()
            {
                confirm_reset = true;
            }
            if game.dirty {
                ui.label(egui::RichText::new("unsaved changes").small().color(egui::Color32::from_rgb(232, 179, 74)));
            }
            if running {
                ui.label(egui::RichText::new("The game is running.").small().color(egui::Color32::from_rgb(232, 106, 106)));
            }
        });
        ui.label(egui::RichText::new(&game.note).small().color(egui::Color32::GRAY));
        // Display output, V-Sync and the frame cap are Nullifier's, because the
        // title ignores its own settings.xml values for them.
        let mut output = self.settings.output.clone();
        let mut vsync = self.settings.sets.get("VICTORY_VSYNC").cloned();
        let mut cap = self
            .settings
            .sets
            .get("VICTORY_FPS_CAP")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);
        let mut layer_changed = false;
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.spacing_mut().interact_size.y = 28.0;
            card(ui, |ui| {
                section_header(ui, "Display");
                ui.horizontal(|ui| {
                    ui.label("Resolution");
                    ui.label(
                        egui::RichText::new("· applied by Nullifier")
                            .small()
                            .color(egui::Color32::from_rgb(120, 126, 138)),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let current = match output.as_str() {
                            "1280x720" => "720p".to_string(),
                            "1920x1080" => "1080p".to_string(),
                            "2560x1440" => "1440p".to_string(),
                            "" => "4K (default)".to_string(),
                            other => format!("Custom ({other})"),
                        };
                        let mut chosen: Option<String> = None;
                        egui::ComboBox::from_id_salt("graphics output")
                            .selected_text(current)
                            .width(150.0)
                            .show_ui(ui, |ui| {
                                for (label, value) in [
                                    ("720p", "1280x720"),
                                    ("1080p", "1920x1080"),
                                    ("1440p", "2560x1440"),
                                    ("4K (default)", ""),
                                ] {
                                    if ui
                                        .selectable_label(output == value, label)
                                        .clicked()
                                    {
                                        chosen = Some(value.to_string());
                                    }
                                }
                            });
                        if let Some(value) = chosen {
                            output = value;
                            layer_changed = true;
                        }
                    });
                });
                ui.horizontal(|ui| {
                    ui.label("V-Sync");
                    ui.label(
                        egui::RichText::new("· applied by Nullifier")
                            .small()
                            .color(egui::Color32::from_rgb(120, 126, 138)),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let current = match vsync.as_deref() {
                            Some("2") => "Adaptive",
                            Some(value)
                                if value != "0" && !value.eq_ignore_ascii_case("off") =>
                            {
                                "On"
                            }
                            _ => "Off",
                        };
                        let mut chosen: Option<Option<String>> = None;
                        egui::ComboBox::from_id_salt("graphics vsync")
                            .selected_text(current)
                            .width(150.0)
                            .show_ui(ui, |ui| {
                                for (label, value) in [
                                    ("Off", ""),
                                    ("On", "1"),
                                    ("Adaptive (same as on)", "2"),
                                ] {
                                    let is = match (vsync.as_deref(), value) {
                                        (None, "") => true,
                                        (Some("0"), "") => true,
                                        (Some(v), "") => v.eq_ignore_ascii_case("off"),
                                        (Some(v), want) => v == want,
                                        _ => false,
                                    };
                                    if ui.selectable_label(is, label).clicked() {
                                        chosen = Some(
                                            (!value.is_empty()).then(|| value.to_string()),
                                        );
                                    }
                                }
                            });
                        if let Some(value) = chosen {
                            vsync = value;
                            layer_changed = true;
                        }
                    });
                });
                ui.horizontal(|ui| {
                    ui.label("Frame Rate Limit");
                    ui.label(
                        egui::RichText::new("· applied by Nullifier")
                            .small()
                            .color(egui::Color32::from_rgb(120, 126, 138)),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let current = match cap {
                            0 => "Unlimited".to_string(),
                            30 | 60 | 120 | 144 | 240 => format!("{cap} fps"),
                            other => format!("Custom ({other} fps)"),
                        };
                        let mut chosen: Option<u32> = None;
                        egui::ComboBox::from_id_salt("graphics frame cap")
                            .selected_text(current)
                            .width(150.0)
                            .show_ui(ui, |ui| {
                                for (label, value) in [
                                    ("Unlimited", 0),
                                    ("30 fps", 30),
                                    ("60 fps", 60),
                                    ("120 fps", 120),
                                    ("144 fps", 144),
                                    ("240 fps", 240),
                                ] {
                                    if ui.selectable_label(cap == value, label).clicked() {
                                        chosen = Some(value);
                                    }
                                }
                            });
                        if let Some(value) = chosen {
                            cap = value;
                            layer_changed = true;
                        }
                    });
                });
                ui.add_space(4.0);
                render_pairs(ui, game, DISPLAY_KEYS);
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(
                            "These three are saved as soon as you pick them; the title ignores its own values for them.",
                        )
                        .small()
                        .color(egui::Color32::from_rgb(120, 126, 138)),
                    )
                    .wrap(),
                );
            });
            ui.add_space(6.0);
            card(ui, |ui| {
                section_header(ui, "Graphics");
                render_pairs(ui, game, GRAPHICS_KEYS);
            });
            ui.add_space(6.0);
            card(ui, |ui| {
                section_header(ui, "Shadows and water");
                render_pairs(ui, game, SHADOW_KEYS);
            });

            // Everything the game writes that is not in the short lists above.
            let rest: Vec<usize> = game
                .settings
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    !s.key.starts_with("version")
                        && !is_curated(&s.section, &s.key)
                        && !s.key.is_empty()
                })
                .map(|(i, _)| i)
                .collect();
            ui.add_space(14.0);
            let arrow = if advanced_open { "\u{25be}" } else { "\u{25b8}" };
            let button = egui::Button::new(
                egui::RichText::new(format!(
                    "{arrow}  Advanced graphics settings  ({})",
                    rest.len()
                ))
                .strong(),
            )
            .fill(egui::Color32::from_rgb(32, 35, 42))
            .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(55, 60, 70)))
            .min_size(egui::vec2(ui.available_width(), 36.0));
            if ui.add(button).clicked() {
                advanced_open = !advanced_open;
            }
            if advanced_open {
                ui.add_space(4.0);
                card(ui, |ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(
                                "Less common values, mostly for troubleshooting. The defaults are a good starting point.",
                            )
                            .small()
                            .color(egui::Color32::GRAY),
                        )
                        .wrap(),
                    );
                    let mut current = String::new();
                    for index in rest {
                        let section = game.settings[index].section.clone();
                        if section != current {
                            if !current.is_empty() {
                                ui.add_space(8.0);
                            }
                            current = section.clone();
                            section_header(ui, section_title(&section));
                        }
                        render_index(ui, game, index);
                    }
                });
            }

            ui.add_space(14.0);
            egui::Frame::group(ui.style())
                .fill(egui::Color32::from_rgb(28, 31, 37))
                .inner_margin(egui::Margin::symmetric(14, 12))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new("Not available yet").strong());
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(
                                "These need engine work and are not wired up yet.",
                            )
                            .small()
                            .color(egui::Color32::GRAY),
                        )
                        .wrap(),
                    );
                    ui.add_space(4.0);
                    ui.add_enabled(false, egui::Checkbox::new(&mut planned.fsr, "FSR upscaling"));
                    ui.add_enabled(false, egui::Checkbox::new(&mut planned.ray_tracing, "Ray-traced light"));
                });
        });
        if layer_changed {
            self.settings.output = output;
            match vsync.as_deref() {
                Some("1") | Some("2") => {
                    self.settings
                        .sets
                        .insert("VICTORY_VSYNC".into(), vsync.clone().unwrap_or_default());
                }
                _ => {
                    self.settings.sets.remove("VICTORY_VSYNC");
                }
            }
            if cap == 0 {
                self.settings.sets.remove("VICTORY_FPS_CAP");
            } else {
                self.settings
                    .sets
                    .insert("VICTORY_FPS_CAP".into(), cap.to_string());
            }
            self.settings.save();
        }
        self.game_advanced_open = advanced_open;
        self.confirm_reset = confirm_reset;
        if reset {
            self.game = None;
            self.status =
                "graphics reset - the game writes fresh defaults on its next start".into();
        } else if reload {
            self.game = load_game_settings();
        }
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        page_header(
            ui,
            "Controls",
            "A connected Xbox or DualSense pad is used automatically; the keyboard stands in when there is none.",
        );
        if !self.controllers_checked {
            self.controllers = scan_controllers();
            self.controllers_checked = true;
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.spacing_mut().interact_size.y = 28.0;
            card(ui, |ui| {
                ui.strong("Input mode");
                ui.label(
                    egui::RichText::new(
                        "Auto is right for almost everyone. Pick a mode only if a pad and the keyboard are both connected and you want one of them ignored.",
                    )
                    .small()
                    .color(egui::Color32::GRAY),
                );
                ui.add_space(4.0);
                let modes = [
                    (
                        "auto",
                        "Auto",
                        "Use a controller when one is connected, otherwise the keyboard.",
                    ),
                    (
                        "controller",
                        "Controller only",
                        "Ignore the keyboard and mouse look; a pad must be connected.",
                    ),
                    (
                        "keyboard",
                        "Mouse & keyboard",
                        "Ignore any pad and always use the keyboard bindings below.",
                    ),
                ];
                let mut chosen = self.settings.input_mode.clone();
                let gap = 10.0;
                let width = ((ui.available_width() - gap * 2.0) / 3.0).max(80.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for (value, title, hint) in modes {
                        let selected = self.settings.input_mode == value;
                        let response = ui.add_sized(
                            [width, 32.0],
                            egui::Button::new(title).selected(selected),
                        );
                        if response.clicked() {
                            chosen = value.to_string();
                        }
                        response.on_hover_text(hint);
                    }
                });
                ui.label(
                    egui::RichText::new(
                        modes
                            .iter()
                            .find(|(value, _, _)| *value == chosen.as_str())
                            .map(|(_, _, hint)| *hint)
                            .unwrap_or(""),
                    )
                    .small()
                    .color(egui::Color32::from_rgb(120, 126, 138)),
                );
                if chosen != self.settings.input_mode {
                    self.settings.input_mode = chosen;
                    self.settings.save();
                    self.status = format!("input mode: {}", self.settings.input_mode);
                }
            });

            ui.add_space(4.0);
            card(ui, |ui| {
                ui.strong("Test input");
                ui.label(
                    egui::RichText::new(
                        "Try your bindings here before starting the game. Pad buttons are read by the engine, so they are tested in-game.",
                    )
                    .small()
                    .color(egui::Color32::GRAY),
                );
                ui.add_space(4.0);
                let mode = self.settings.input_mode.as_str();
                let keys_used = mode != "controller";
                let pad_used = mode != "keyboard";

                let mut seen_key = self.test_key.clone();
                if keys_used {
                    for event in ui.input(|i| i.events.clone()) {
                        if let egui::Event::Key {
                            key,
                            pressed: true,
                            repeat: false,
                            ..
                        } = event
                        {
                            seen_key = Some(key_name(key));
                        }
                    }
                    self.test_key = seen_key.clone();
                }
                let delta = ui.input(|i| i.pointer.delta());
                if delta != egui::Vec2::ZERO {
                    self.test_mouse = delta;
                }

                egui::Grid::new("input test")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.label(small_label("Pad"));
                        if self.controllers.is_empty() {
                            ui.label(
                                egui::RichText::new("none connected").color(
                                    egui::Color32::from_rgb(232, 179, 74),
                                ),
                            );
                        } else if pad_used {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} — in use",
                                    self.controllers.join(", ")
                                ))
                                .color(egui::Color32::from_rgb(87, 199, 133)),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} — ignored by this mode",
                                    self.controllers.join(", ")
                                ))
                                .color(egui::Color32::from_rgb(120, 126, 138)),
                            );
                        }
                        ui.end_row();

                        ui.label(small_label("Keyboard"));
                        if !keys_used {
                            ui.label(
                                egui::RichText::new("ignored by this mode")
                                    .color(egui::Color32::from_rgb(120, 126, 138)),
                            );
                        } else if let Some(key) = &seen_key {
                            let action = self
                                .keyboard_keys
                                .iter()
                                .position(|bound| same_key(bound, key))
                                .map(|index| PAD_ACTIONS[index].0);
                            match action {
                                Some(action) => {
                                    ui.label(egui::RichText::new(format!(
                                        "{key} → pad {action}"
                                    )));
                                }
                                None => {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{key} — not bound to a pad button"
                                        ))
                                        .color(egui::Color32::from_rgb(120, 126, 138)),
                                    );
                                }
                            }
                        } else {
                            ui.label(
                                egui::RichText::new("press a key")
                                    .color(egui::Color32::from_rgb(120, 126, 138)),
                            );
                        }
                        ui.end_row();

                        ui.label(small_label("Mouse"));
                        if !keys_used || !self.settings.mouse_look {
                            ui.label(
                                egui::RichText::new(if !keys_used {
                                    "ignored by this mode".to_string()
                                } else {
                                    "mouse look is off".to_string()
                                })
                                .color(egui::Color32::from_rgb(120, 126, 138)),
                            );
                        } else if self.test_mouse == egui::Vec2::ZERO {
                            ui.label(
                                egui::RichText::new("move the pointer to test")
                                    .color(egui::Color32::from_rgb(120, 126, 138)),
                            );
                        } else {
                            ui.label(format!(
                                "{:+.0} × {:+.0} px → right stick {:.2}, {:.2}",
                                self.test_mouse.x,
                                self.test_mouse.y,
                                (self.test_mouse.x * self.settings.mouse_sensitivity)
                                    .clamp(-1.0, 1.0),
                                (self.test_mouse.y * self.settings.mouse_sensitivity
                                    * if self.settings.mouse_invert { -1.0 } else { 1.0 })
                                    .clamp(-1.0, 1.0),
                            ));
                        }
                        ui.end_row();

                        if keys_used && self.settings.mouse_look && self.settings.mouse_buttons {
                            let (fire, aim) = ui.input(|i| {
                                (
                                    i.pointer.button_down(egui::PointerButton::Primary),
                                    i.pointer.button_down(egui::PointerButton::Secondary),
                                )
                            });
                            ui.label(small_label("Buttons"));
                            ui.label(format!(
                                "left {}{} · right {}{}",
                                if fire { "→ fire" } else { "= fire" },
                                if fire { " (held)" } else { "" },
                                if aim { "→ aim" } else { "= aim" },
                                if aim { " (held)" } else { "" },
                            ));
                            ui.end_row();
                        }

                        ui.label(small_label("Sticks"));
                        ui.label(
                            egui::RichText::new(
                                "W A S D (left) · I J K L (right) · Z C (triggers)",
                            )
                            .color(egui::Color32::from_rgb(120, 126, 138)),
                        );
                        ui.end_row();
                    });
            });

            ui.add_space(4.0);
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("Controllers");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if chip_button(ui, "Check again").clicked() {
                            self.controllers = scan_controllers();
                            self.status = if self.controllers.is_empty() {
                                "no controller found".into()
                            } else {
                                format!("{} controller(s) found", self.controllers.len())
                            };
                        }
                    });
                });
                if self.controllers.is_empty() {
                    ui.label(
                        egui::RichText::new("No controller found.")
                            .color(egui::Color32::from_rgb(232, 179, 74)),
                    );
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(
                                "Plug in an Xbox pad or a DualSense and press Check again. On Linux, a DualSense sometimes needs Steam Input to appear as a pad.",
                            )
                            .small()
                            .color(egui::Color32::GRAY),
                        )
                        .wrap(),
                    );
                } else {
                    for name in &self.controllers {
                        ui.label(
                            egui::RichText::new(format!("•  {name}"))
                                .color(egui::Color32::from_rgb(87, 199, 133)),
                        );
                    }
                }
            });

            ui.add_space(4.0);
            let keys_used = self.settings.input_mode != "controller";
            ui.add_enabled_ui(keys_used, |ui| {
            card(ui, |ui| {
                ui.strong("Keyboard pad");
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(
                            "Keys that act as pad buttons while no controller is connected. Write a key as Enter, Space, F1, or one letter.",
                        )
                        .small()
                        .color(egui::Color32::GRAY),
                    )
                    .wrap(),
                );
                ui.add_space(4.0);
                ui.columns(2, |columns| {
                    for (half, column) in columns.iter_mut().enumerate() {
                        let first = half * 7;
                        egui::Grid::new(format!("keyboard grid {half}"))
                            .num_columns(2)
                            .spacing([12.0, 6.0])
                            .show(column, |ui| {
                                for (index, (action, _)) in
                                    PAD_ACTIONS.iter().enumerate().skip(first).take(7)
                                {
                                    ui.label(*action);
                                    ui.add_sized(
                                        [92.0, 28.0],
                                        egui::TextEdit::singleline(&mut self.keyboard_keys[index])
                                            .vertical_align(egui::Align::Center),
                                    );
                                    ui.end_row();
                                }
                            });
                    }
                });
                if self.keyboard_keys.iter().any(|key| !key_name_ok(key)) {
                    ui.label(
                        egui::RichText::new("A key name is not recognised; the game will ignore that one.")
                            .small()
                            .color(egui::Color32::from_rgb(232, 106, 106)),
                    );
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add(primary("Apply", 80.0, 30.0)).clicked() {
                        self.settings.keyboard = keys_to_text(&self.keyboard_keys);
                        self.settings.save();
                        self.status = "keyboard bindings saved".into();
                    }
                    if ui.add(secondary("Reset to defaults", 130.0, 30.0)).clicked() {
                        self.keyboard_keys = load_keys("");
                        self.settings.keyboard.clear();
                        self.settings.save();
                        self.status = "keyboard bindings reset".into();
                    }
                });
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(
                            "Sticks: W A S D (left) and I J K L (right). Triggers: Z and C. Those stay fixed.",
                        )
                        .small()
                        .color(egui::Color32::GRAY),
                    )
                    .wrap(),
                );
            });
            });

            ui.add_space(4.0);
            ui.add_enabled_ui(keys_used, |ui| {
            card(ui, |ui| {
                ui.strong("Mouse");
                let mut changed = false;
                if ui
                    .checkbox(
                        &mut self.settings.mouse_look,
                        "Mouse look (moves the right stick)",
                    )
                    .changed()
                {
                    changed = true;
                }
                ui.add_enabled_ui(self.settings.mouse_look, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Sensitivity");
                        if ui
                            .add(
                                egui::Slider::new(
                                    &mut self.settings.mouse_sensitivity,
                                    0.005..=0.08,
                                )
                                .fixed_decimals(3),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });
                    if ui
                        .checkbox(&mut self.settings.mouse_invert, "Invert vertical")
                        .changed()
                    {
                        changed = true;
                    }
                    if ui
                        .checkbox(
                            &mut self.settings.mouse_buttons,
                            "Left button fires, right button aims",
                        )
                        .on_hover_text(
                            "Left is the right trigger (RT), right is the left trigger (LT).",
                        )
                        .changed()
                    {
                        changed = true;
                    }
                });
                if changed {
                    self.settings.save();
                }
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(
                            "Experimental. The cursor is hidden and held in the game window; its movement turns the right stick, and the buttons can act as the triggers. Active while no controller is connected, or always in Mouse & keyboard mode. Menu navigation with the mouse is not included.",
                        )
                        .small()
                        .color(egui::Color32::GRAY),
                    )
                    .wrap(),
                );
            });
            });
        });
    }

    fn save_community_after_fetch(&self) {
        save_community(&self.community);
    }

    fn start_community_fetch(&mut self) {
        if self.community_fetching {
            return;
        }
        self.community_fetching = true;
        self.community_status = "Checking the published board...".into();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(fetch_board());
        });
        self.community_rx = Some(rx);
    }

    fn use_settings(&mut self, name: &str, sets: &BTreeMap<String, String>) {
        if sets.is_empty() {
            self.community_status = "That run has no settings saved.".into();
            return;
        }
        if self.borrowed_sets.is_none() {
            self.borrowed_sets = Some(self.settings.sets.clone());
            self.borrowed_profile = Some(self.settings.profile.clone());
        }
        self.settings.sets = sets.clone();
        self.settings.profile = "custom".into();
        self.settings.save();
        self.borrowed_from = Some(name.to_string());
        self.tab = Tab::Play;
        self.status = format!("using {name}'s settings");
    }

    fn keep_settings(&mut self) {
        self.borrowed_sets = None;
        self.borrowed_profile = None;
        self.borrowed_from = None;
        self.settings.save();
        self.status = "settings kept".into();
    }

    fn revert_settings(&mut self) {
        if let Some(sets) = self.borrowed_sets.take() {
            self.settings.sets = sets;
        }
        if let Some(profile) = self.borrowed_profile.take() {
            self.settings.profile = profile;
        }
        self.borrowed_from = None;
        self.settings.save();
        self.status = "your settings are back".into();
    }

    fn share_run(&mut self) -> Option<String> {
        let Some(path) = self.log_path.clone() else {
            self.community_status = "No run log yet - play or measure once first.".into();
            return None;
        };
        let Ok(raw) = std::fs::read_to_string(&path) else {
            self.community_status = "Could not read the last log.".into();
            return None;
        };
        let name = self.settings.community_name.trim().to_string();
        if name.is_empty() {
            self.community_status = "Choose a name first (a nickname is fine).".into();
            return None;
        }
        let (world_fps, _, _, _) = world_stats(&raw);
        if world_fps.is_none() {
            self.community_status =
                "The last log has no in-game frames (menus run at 30). Play or measure in the world first.".into();
            return None;
        }
        let log = sanitize_log(&raw, &self.settings);
        let (fps, frame, gpu, draws) = world_stats(&raw);
        let fps = fps.unwrap_or(0.0);
        let when = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let entry = CommunityEntry {
            name: name.clone(),
            version: LAYER_VERSION.to_string(),
            game: game_label(&raw),
            profile: if self.settings.profile.is_empty() {
                "custom".into()
            } else {
                self.settings.profile.clone()
            },
            fps,
            frame_ms: frame.unwrap_or(0.0),
            gpu_ms: gpu.unwrap_or(0.0),
            draws: draws.unwrap_or(0),
            area: self.settings.community_area.trim().to_string(),
            notes: self.settings.community_notes.trim().to_string(),
            settings: settings_summary(&self.settings, &self.hardware),
            sets: self.settings.sets.clone(),
            launcher: VERSION.to_string(),
            hardware: format!(
                "{} {}, {}",
                self.hardware.vendor.to_uppercase(),
                vram_label(self.hardware.vram_mb),
                std::env::consts::OS
            ),
            when,
            log: log.clone(),
            remote: false,
        };
        let game = entry.game.clone();
        let profile = entry.profile.clone();
        let frame_ms = entry.frame_ms;
        let gpu_ms = entry.gpu_ms;
        let draws = entry.draws;
        let area = entry.area.clone();
        let notes = entry.notes.clone();
        let settings_line = entry.settings.clone();
        let hardware = entry.hardware.clone();
        self.community.insert(0, entry);
        save_community(&self.community);
        self.community_status = format!("Added {name}'s run; a share block is on the clipboard.");
        let json = serde_json::to_string_pretty(&CommunityEntry {
            name: name.clone(),
            version: LAYER_VERSION.to_string(),
            game,
            profile: profile.clone(),
            fps,
            frame_ms,
            gpu_ms,
            draws,
            area,
            notes,
            settings: settings_line,
            sets: self.settings.sets.clone(),
            hardware,
            launcher: VERSION.to_string(),
            when,
            log,
            remote: false,
        })
        .unwrap_or_default();
        Some(format!(
            "**{name}** · Nullifier v{VERSION} · **{fps:.1} fps**\n\
             {draws} draws/frame · frame {frame_ms:.1} ms · GPU {gpu_ms:.1} ms · profile {profile}\n\
             Paste this whole block in the GitHub form.\n\
             \n```json\n{json}\n```\n"
        ))
    }

    fn community_page(&mut self, ui: &mut egui::Ui) {
        if !self.community_checked {
            self.community_checked = true;
            self.start_community_fetch();
        }
        page_header(
            ui,
            "Community",
            "Share how it runs. Nothing leaves this PC unless you paste the copied block.",
        );
        // Fields need to read as fields on this dark page.
        ui.visuals_mut().extreme_bg_color = egui::Color32::from_rgb(30, 33, 40);
        ui.visuals_mut().widgets.inactive.bg_stroke =
            egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(60, 66, 78));

        egui::ScrollArea::vertical().show(ui, |ui| {
            card(ui, |ui| {
                ui.strong("Your entry");
                let mut changed = false;
                egui::Grid::new("community entry")
                    .num_columns(2)
                    .min_row_height(30.0)
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        ui.label(small_label("Username"));
                        changed |= ui
                            .add_sized(
                                [380.0, 28.0],
                                egui::TextEdit::singleline(
                                    &mut self.settings.community_name,
                                )
                                .hint_text("a nickname")
                                .vertical_align(egui::Align::Center),
                            )
                            .changed();
                        ui.end_row();
                        ui.label(small_label("Where"));
                        changed |= ui
                            .add_sized(
                                [380.0, 28.0],
                                egui::TextEdit::singleline(
                                    &mut self.settings.community_area,
                                )
                                .hint_text("prologue cell, boat mission…")
                                .vertical_align(egui::Align::Center),
                            )
                            .changed();
                        ui.end_row();
                        ui.label(small_label("Notes"));
                        changed |= ui
                            .add_sized(
                                [380.0, 28.0],
                                egui::TextEdit::singleline(
                                    &mut self.settings.community_notes,
                                )
                                .hint_text("pop-in, flicker, HUD…")
                                .vertical_align(egui::Align::Center),
                            )
                            .changed();
                        ui.end_row();
                    });
                if changed {
                    self.settings.save();
                }
                ui.add_space(6.0);
                let (world_fps, _, _, world_draws) = world_stats(&self.log_shown);
                let in_world = world_fps.is_some();
                let run = self.last_run();
                let fps = if in_world { world_fps } else { run.fps };
                let draws = world_draws.or(run.draws).unwrap_or(0);
                ui.horizontal(|ui| {
                    let can = self.log_path.is_some()
                        && !self.settings.community_name.trim().is_empty()
                        && !self.running();
                    if ui
                        .add_enabled(can, primary("Share last run", 140.0, 32.0))
                        .clicked()
                    {
                        if let Some(markdown) = self.share_run() {
                            ui.ctx().copy_text(markdown);
                        }
                    }
                    if let Some(fps) = fps {
                        ui.label(
                            egui::RichText::new(format!(
                                "{fps:.1} fps {} · {draws} draws · {}",
                                if in_world { "in the world" } else { "(last log, maybe a menu)" },
                                if run.removals == 0 {
                                    "no crashes"
                                } else {
                                    "crashes"
                                }
                            ))
                            .small()
                            .color(egui::Color32::GRAY),
                        );
                    }
                });
                if !self.community_status.is_empty() {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&self.community_status)
                                .small()
                                .color(egui::Color32::from_rgb(160, 166, 178)),
                        )
                        .wrap(),
                    );
                }
            });

            ui.add_space(4.0);
            card(ui, |ui| {
                let local = self.community.iter().filter(|entry| !entry.remote).count();
                let remote = self.community.len() - local;
                ui.horizontal(|ui| {
                    ui.strong("Shared runs");
                    ui.label(
                        egui::RichText::new(format!("{local} local · {remote} GitHub"))
                            .small()
                            .color(egui::Color32::GRAY),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.community_fetching {
                            ui.add_enabled(false, egui::Button::new("Checking board…").small());
                        } else if ui.add(secondary("Refresh board", 120.0, 28.0)).clicked() {
                            self.start_community_fetch();
                        }
                    });
                });
                ui.add_space(4.0);
                let mut games: Vec<String> =
                    self.community.iter().map(|entry| entry.game.clone()).collect();
                games.sort();
                games.dedup();
                let mut areas: Vec<String> = self
                    .community
                    .iter()
                    .map(|entry| entry.area.clone())
                    .filter(|area| !area.is_empty())
                    .collect();
                areas.sort();
                areas.dedup();
                let mut versions: Vec<String> =
                    self.community.iter().map(|entry| entry.version.clone()).collect();
                versions.sort();
                versions.dedup();
                let active_filters = usize::from(!self.community_game.is_empty())
                    + usize::from(!self.community_area_filter.is_empty())
                    + usize::from(!self.community_version_filter.is_empty())
                    + usize::from(self.community_notes_only);
                ui.horizontal(|ui| {
                    ui.spacing_mut().interact_size.y = 28.0;
                    let search_width = (ui.available_width() - 230.0).max(180.0);
                    ui.add_sized(
                        [search_width, 28.0],
                        egui::TextEdit::singleline(&mut self.community_filter)
                            .hint_text("Search")
                            .vertical_align(egui::Align::Center),
                    );
                    let filters_label = if active_filters == 0 {
                        "Filters".to_string()
                    } else {
                        format!("Filters ({active_filters})")
                    };
                    ui.menu_button(filters_label, |ui| {
                        ui.set_min_width(240.0);
                        ui.spacing_mut().interact_size.y = 28.0;
                        ui.label(small_label("Game"));
                        let current = if self.community_game.is_empty() {
                            "All".to_string()
                        } else {
                            self.community_game.clone()
                        };
                        egui::ComboBox::from_id_salt("community game")
                            .selected_text(current)
                            .width(220.0)
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(self.community_game.is_empty(), "All")
                                    .clicked()
                                {
                                    self.community_game.clear();
                                }
                                for game in &games {
                                    if ui
                                        .selectable_label(self.community_game == *game, game)
                                        .clicked()
                                    {
                                        self.community_game = game.clone();
                                    }
                                }
                            });
                        ui.label(small_label("Area"));
                        let current = if self.community_area_filter.is_empty() {
                            "All".to_string()
                        } else {
                            self.community_area_filter.clone()
                        };
                        egui::ComboBox::from_id_salt("community area")
                            .selected_text(current)
                            .width(220.0)
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(self.community_area_filter.is_empty(), "All")
                                    .clicked()
                                {
                                    self.community_area_filter.clear();
                                }
                                for area in &areas {
                                    if ui
                                        .selectable_label(self.community_area_filter == *area, area)
                                        .clicked()
                                    {
                                        self.community_area_filter = area.clone();
                                    }
                                }
                            });
                        ui.label(small_label("Version"));
                        let current = if self.community_version_filter.is_empty() {
                            "All".to_string()
                        } else {
                            format!("v{}", self.community_version_filter)
                        };
                        egui::ComboBox::from_id_salt("community version")
                            .selected_text(current)
                            .width(220.0)
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(
                                        self.community_version_filter.is_empty(),
                                        "All",
                                    )
                                    .clicked()
                                {
                                    self.community_version_filter.clear();
                                }
                                for version in &versions {
                                    if ui
                                        .selectable_label(
                                            self.community_version_filter == *version,
                                            format!("v{version}"),
                                        )
                                        .clicked()
                                    {
                                        self.community_version_filter = version.clone();
                                    }
                                }
                            });
                        ui.add_space(4.0);
                        ui.checkbox(&mut self.community_notes_only, "Only runs with notes");
                        if active_filters > 0 && ui.button("Clear filters").clicked() {
                            self.community_game.clear();
                            self.community_area_filter.clear();
                            self.community_version_filter.clear();
                            self.community_notes_only = false;
                            ui.close_menu();
                        }
                    });
                    let sort_text = if self.community_sort == 1 { "Fastest" } else { "Newest" };
                    egui::ComboBox::from_id_salt("community sort")
                        .selected_text(sort_text)
                        .width(95.0)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(self.community_sort == 0, "Newest")
                                .clicked()
                            {
                                self.community_sort = 0;
                            }
                            if ui
                                .selectable_label(self.community_sort == 1, "Fastest")
                                .clicked()
                            {
                                self.community_sort = 1;
                            }
                        });
                });
                ui.add_space(4.0);
                let needle = self.community_filter.to_lowercase();
                let mut shown: Vec<usize> = self
                    .community
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| {
                        (self.community_game.is_empty() || e.game == self.community_game)
                            && (self.community_area_filter.is_empty()
                                || e.area == self.community_area_filter)
                            && (self.community_version_filter.is_empty()
                                || e.version == self.community_version_filter)
                            && (!self.community_notes_only || !e.notes.is_empty())
                            && (needle.is_empty()
                                || e.name.to_lowercase().contains(&needle)
                                || e.game.to_lowercase().contains(&needle)
                                || e.profile.to_lowercase().contains(&needle)
                                || e.area.to_lowercase().contains(&needle)
                                || e.notes.to_lowercase().contains(&needle)
                                || e.settings.to_lowercase().contains(&needle))
                    })
                    .map(|(index, _)| index)
                    .collect();
                if self.community_sort == 1 {
                    let entries = &self.community;
                    shown.sort_by(|a, b| {
                        entries[*b]
                            .fps
                            .partial_cmp(&entries[*a].fps)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                }
                let mut open = self.community_open;
                let mut borrow: Option<(String, BTreeMap<String, String>)> = None;
                if shown.is_empty() {
                    ui.label(
                        egui::RichText::new(if self.community.is_empty() {
                            "Nothing shared yet. Play once, then press Share last run."
                        } else {
                            "No run matches the filter."
                        })
                        .color(egui::Color32::GRAY),
                    );
                } else {
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.add_space(10.0);
                        table_cell(ui, 140.0, |ui| {
                            ui.label(small_label("Username"));
                        });
                        table_sep(ui);
                        table_cell(ui, 56.0, |ui| {
                            ui.label(small_label("Version"));
                        });
                        table_sep(ui);
                        table_cell(ui, 64.0, |ui| {
                            ui.label(small_label("FPS"));
                        });
                        table_sep(ui);
                        ui.add_space(6.0);
                        ui.label(small_label("Where"));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_space(50.0);
                        });
                    });
                    ui.separator();
                }
                {
                    {
                        for (row, index) in shown.into_iter().enumerate() {
                            let entry = &self.community[index];
                            let stripe = if row % 2 == 0 {
                                egui::Color32::from_rgb(27, 30, 36)
                            } else {
                                egui::Color32::TRANSPARENT
                            };
                            egui::Frame::group(ui.style())
                                .fill(stripe)
                                .inner_margin(egui::Margin::symmetric(10, 8))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        table_cell(ui, 140.0, |ui| {
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(&entry.name).strong(),
                                                )
                                                .truncate(),
                                            );
                                        });
                                        table_sep(ui);
                                        table_cell(ui, 56.0, |ui| {
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(format!(
                                                        "v{}",
                                                        entry.version
                                                    ))
                                                    .small()
                                                    .color(egui::Color32::GRAY),
                                                )
                                                .truncate(),
                                            );
                                        });
                                        table_sep(ui);
                                        table_cell(ui, 64.0, |ui| {
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(format!(
                                                        "{:.1} fps",
                                                        entry.fps
                                                    ))
                                                    .strong()
                                                    .color(egui::Color32::from_rgb(
                                                        87, 199, 133,
                                                    )),
                                                )
                                                .truncate(),
                                            );
                                        });
                                        table_sep(ui);
                                        ui.add_space(6.0);
                                        let where_at = if entry.area.is_empty() {
                                            entry.game.clone()
                                        } else {
                                            entry.area.clone()
                                        };
                                        let area_width =
                                            (ui.available_width() - 78.0).max(80.0);
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(area_width, 20.0),
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                ui.add(
                                                    egui::Label::new(
                                                        egui::RichText::new(where_at).small(),
                                                    )
                                                    .truncate(),
                                                );
                                            },
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if chip_button(ui, "Log").clicked() {
                                                    open = Some(index);
                                                }
                                                if entry.name != self.settings.community_name
                                                    && !entry.sets.is_empty()
                                                    && chip_button(ui, "Use")
                                                        .on_hover_text(
                                                            "Try this run's settings; Revert on Play brings yours back.",
                                                        )
                                                        .clicked()
                                                {
                                                    borrow = Some((
                                                        entry.name.clone(),
                                                        entry.sets.clone(),
                                                    ));
                                                }
                                                if entry.remote {
                                                    ui.label(
                                                        egui::RichText::new("GitHub")
                                                            .small()
                                                            .color(egui::Color32::from_rgb(
                                                                120, 165, 240,
                                                            )),
                                                    );
                                                }
                                            },
                                        );
                                    });
                                    ui.horizontal(|ui| {
                                        ui.add_sized(
                                            [190.0, 16.0],
                                            egui::Label::new(
                                                egui::RichText::new(format!(
                                                    "{} · {}",
                                                    entry.profile, entry.game
                                                ))
                                                .small()
                                                .color(egui::Color32::GRAY),
                                            )
                                            .truncate(),
                                        );
                                        if !entry.notes.is_empty() {
                                            let notes: String = if entry.notes.chars().count() > 90 {
                                                format!(
                                                    "{}…",
                                                    entry.notes.chars().take(90).collect::<String>()
                                                )
                                            } else {
                                                entry.notes.clone()
                                            };
                                            ui.add_sized(
                                                [ui.available_width() - 78.0, 16.0],
                                                egui::Label::new(
                                                    egui::RichText::new(notes)
                                                        .small()
                                                        .color(egui::Color32::from_rgb(
                                                            232, 179, 74,
                                                        )),
                                                )
                                                .truncate(),
                                            );
                                        }
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                ui.label(
                                                    egui::RichText::new(date_of(entry.when))
                                                        .small()
                                                        .color(egui::Color32::GRAY),
                                                );
                                            },
                                        );
                                    });
                                });
                        }
                    }
                }
                if let Some((name, sets)) = borrow {
                    self.use_settings(&name, &sets);
                }
                self.community_open = open;
            });
        });

        if let Some(index) = self.community_open {
            let mut open = true;
            let mut remove = false;
            let mut modal_use: Option<(String, BTreeMap<String, String>)> = None;
            if let Some(entry) = self.community.get(index) {
                let title = format!("{} · {:.1} fps · {}", entry.name, entry.fps, entry.game);
                egui::Window::new(title)
                    .open(&mut open)
                    .collapsible(false)
                    .default_size([640.0, 460.0])
                    .show(ui.ctx(), |ui| {
                        ui.horizontal(|ui| {
                            if chip_button(ui, "Copy log").clicked() {
                                ui.ctx().copy_text(entry.log.clone());
                            }
                            if entry.name != self.settings.community_name
                                && !entry.sets.is_empty()
                                && chip_button(ui, "Use settings").clicked()
                            {
                                modal_use = Some((entry.name.clone(), entry.sets.clone()));
                            }
                            if !entry.remote && chip_button(ui, "Remove").clicked() {
                                remove = true;
                            }
                            ui.label(
                                egui::RichText::new(format!(
                                    "Layer v{} · launcher v{} · profile {} · {} draws · frame {:.1} ms · GPU {:.1} ms",
                                    entry.version,
                                    if entry.launcher.is_empty() { "-" } else { &entry.launcher },
                                    entry.profile,
                                    entry.draws,
                                    entry.frame_ms,
                                    entry.gpu_ms
                                ))
                                .small()
                                .color(egui::Color32::GRAY),
                            );
                        });
                        if !entry.area.is_empty() {
                            ui.label(format!("Where: {}", entry.area));
                        }
                        if !entry.notes.is_empty() {
                            ui.label(
                                egui::RichText::new(format!("Notes: {}", entry.notes))
                                    .color(egui::Color32::from_rgb(232, 179, 74)),
                            );
                        }
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(format!("Hardware: {}", entry.hardware))
                                    .small()
                                    .color(egui::Color32::GRAY),
                            )
                            .wrap(),
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(format!("Settings: {}", entry.settings))
                                    .small()
                                    .color(egui::Color32::GRAY),
                            )
                            .wrap(),
                        );
                        ui.separator();
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            ui.monospace(&entry.log);
                        });
                    });
            }
            if let Some((name, sets)) = modal_use {
                self.use_settings(&name, &sets);
                self.community_open = None;
            } else if remove {
                self.community.remove(index);
                save_community(&self.community);
                self.community_open = None;
            } else if !open {
                self.community_open = None;
            }
        }
    }

    fn rendering(&mut self, ui: &mut egui::Ui) {
        page_header(
            ui,
            "Rendering",
            "Nullifier's own rendering settings: speed, memory and stability. A ? explains any of them.",
        );
        let warn = warnings(&self.settings, &self.hardware);
        warnings_card(ui, &warn);
        let mut info = None;
        let mut render_scale = self.settings.render_scale;
        let mut display_changed = false;
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.spacing_mut().interact_size.y = 28.0;
            let sets = &mut self.settings.sets;
            card(ui, |ui| {
                ui.strong("Display");
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(
                            "Resolution, V-Sync and the frame rate limit are on Graphics; Nullifier applies them. Render scale is the title's own upscaler.",
                        )
                        .small()
                        .color(egui::Color32::GRAY),
                    )
                    .wrap(),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Render scale");
                    let current = if render_scale > 0.0 {
                        "1.0 (native)"
                    } else {
                        "Title default (2/3)"
                    };
                    egui::ComboBox::from_id_salt("render scale")
                        .selected_text(current)
                        .width(160.0)
                        .show_ui(ui, |ui| {
                            for (label, value) in
                                [("Title default (2/3)", 0.0_f32), ("1.0 (native)", 1.0)]
                            {
                                if ui
                                    .selectable_label(
                                        (render_scale - value).abs() < 0.001,
                                        label,
                                    )
                                    .clicked()
                                {
                                    render_scale = value;
                                    display_changed = true;
                                }
                            }
                        });
                });
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(
                            "At 1.0 the title renders at the output size: sharper and slower.",
                        )
                        .small()
                        .color(egui::Color32::from_rgb(120, 126, 138)),
                    )
                    .wrap(),
                );
            });
            ui.add_space(4.0);
            card(ui, |ui| {
                ui.strong("Memory and copies");
                let asked = number_info(ui, sets, "VICTORY_TEXTURE_BUDGET_MB", 0..=8192, 256);
                if info.is_none() { info = asked; }
                let asked = number_info(ui, sets, "VICTORY_CARD_BUFFERS_MB", 0..=4096, 64);
                if info.is_none() { info = asked; }
                let asked = switch_info(ui, sets, "VICTORY_TEXTURE_NO_WAIT");
                if info.is_none() { info = asked; }
                let asked = switch_info(ui, sets, "VICTORY_CARD_HASH_LANES");
                if info.is_none() {
                    info = asked;
                }
                let asked = switch_info(ui, sets, "VICTORY_WIDE_BACK");
                if info.is_none() { info = asked; }
                let asked = switch_info(ui, sets, "VICTORY_BACK_OWN_VIEW");
                if info.is_none() { info = asked; }
                let asked = switch_info(ui, sets, "VICTORY_ALIAS_LAZY");
                if info.is_none() { info = asked; }
                let asked = switch_info(ui, sets, "VICTORY_CARD_REHASH_RUNS");
                if info.is_none() { info = asked; }
            });
            ui.add_space(4.0);
            card(ui, |ui| {
                ui.strong("Draw path");
                let asked = switch_info(ui, sets, "VICTORY_INDIRECT_WINDOW");
                if info.is_none() { info = asked; }
                let asked = switch_info(ui, sets, "VICTORY_INDIRECT_RUNS");
                if info.is_none() { info = asked; }
            });
            ui.add_space(4.0);
            card(ui, |ui| {
                ui.strong("Timing");
                let asked = number_info(ui, sets, "VICTORY_LATE_WAIT", 0..=3, 1);
                if info.is_none() { info = asked; }
                let asked = switch_info(ui, sets, "VICTORY_JOIN_COPIES");
                if info.is_none() { info = asked; }
                let asked = number_info(ui, sets, "VICTORY_RELAY", 0..=2, 2);
                if info.is_none() { info = asked; }
                let asked = switch_info(ui, sets, "VICTORY_RELAY_QUEUE");
                if info.is_none() { info = asked; }
            });
        });
        if display_changed {
            self.settings.render_scale = render_scale;
            self.settings.save();
        }
        if let Some(key) = info {
            self.info = Some(key);
        }
    }

    fn advanced(&mut self, ui: &mut egui::Ui) {
        page_header(ui, "Advanced", "Exact settings and environment, for troubleshooting.");
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.spacing_mut().interact_size.y = 28.0;
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("All settings");
                    let mut add = None;
                    ui.menu_button("Add a setting…", |ui| {
                        let mut any = false;
                        for (key, value) in KNOWN_SWITCHES {
                            if self.settings.sets.contains_key(*key) {
                                continue;
                            }
                            any = true;
                            let label = describe(key).map(|d| d.name).unwrap_or(key);
                            if ui.button(format!("{label}  ({key})")).clicked() {
                                add = Some((key.to_string(), value.to_string()));
                                ui.close_menu();
                            }
                        }
                        if !any {
                            ui.label(
                                egui::RichText::new("All known settings are already on.")
                                    .small()
                                    .color(egui::Color32::GRAY),
                            );
                        }
                    });
                    if let Some((key, value)) = add {
                        self.settings.sets.insert(key, value);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new("Missing a switch? Add it below with a custom KEY=VALUE.")
                                .small()
                                .color(egui::Color32::from_rgb(130, 136, 148)),
                        );
                    });
                });
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(
                            "One line per setting; removing it turns it off. Values are the engine's VICTORY_* variables.",
                        )
                        .small()
                        .color(egui::Color32::GRAY),
                    )
                    .wrap(),
                );
                ui.add_space(4.0);
                let mut remove = None;
                let mut info = None;
                let entries: Vec<(String, String)> = self
                    .settings
                    .sets
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                egui::Grid::new("advanced settings")
                    .num_columns(4)
                    .spacing([10.0, 6.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label(small_label("Setting"));
                        ui.label("");
                        ui.label(small_label("Value"));
                        ui.label("");
                        ui.end_row();
                        for (key, value) in entries {
                            ui.allocate_ui_with_layout(
                                egui::vec2(300.0, 28.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.label(egui::RichText::new(&key).monospace());
                                },
                            );
                            if describe(&key).is_some() {
                                if help_button(ui).clicked() {
                                    info = Some(key.clone());
                                }
                            } else {
                                ui.label("");
                            }
                            let mut v = value.clone();
                            let hint = example(&key).unwrap_or("value");
                            if ui
                                .add_sized(
                                    [120.0, 28.0],
                                    egui::TextEdit::singleline(&mut v)
                                        .hint_text(hint)
                                        .vertical_align(egui::Align::Center),
                                )
                                .changed()
                            {
                                self.settings.sets.insert(key.clone(), v);
                            }
                            if danger_remove(ui).clicked() {
                                remove = Some(key.clone());
                            }
                            ui.end_row();
                        }
                    });
                if let Some(key) = remove {
                    self.settings.sets.remove(&key);
                }
                if let Some(key) = info {
                    self.info = Some(key);
                }
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(2.0);
                egui::CollapsingHeader::new("Custom setting (KEY=VALUE)")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.new_key)
                                    .desired_width(240.0)
                                    .hint_text("VICTORY_…"),
                            );
                            ui.add(
                                egui::TextEdit::singleline(&mut self.new_value)
                                    .desired_width(120.0)
                                    .hint_text("e.g. 1 or 2048"),
                            );
                            if ui.button("Add").clicked()
                                && !self.new_key.trim().is_empty()
                            {
                                self.settings.sets.insert(
                                    self.new_key.trim().into(),
                                    self.new_value.trim().into(),
                                );
                                self.new_key.clear();
                                self.new_value.clear();
                            }
                        });
                    });
            });

            ui.add_space(4.0);
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("Environment variables");
                    help_button(ui).on_hover_text(
                        "Extra KEY=VALUE variables passed to the engine at launch. The launcher already sets the profile; use this only to test an override.",
                    );
                });
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(
                            "One KEY=VALUE per line. The launcher already applies the profile — use this only for a temporary override, for example VICTORY_LOG=debug.",
                        )
                        .small()
                        .color(egui::Color32::GRAY),
                    )
                    .wrap(),
                );
                ui.add(
                    egui::TextEdit::multiline(&mut self.settings.extra_env)
                        .desired_rows(3)
                        .hint_text("VICTORY_LOG=debug"),
                );
            });

            ui.add_space(4.0);
            card(ui, |ui| {
                ui.strong("Profile details");
                if let Some(profile) = self.profile() {
                    ui.label(format!("{} · {}", profile.name, profile.confidence));
                    ui.add_space(4.0);
                    let mut entries: Vec<(&str, &str)> = profile
                        .sets
                        .iter()
                        .map(|(key, value)| (key.as_str(), value.as_str()))
                        .collect();
                    entries.sort();
                    egui::Grid::new("profile settings")
                        .num_columns(2)
                        .spacing([18.0, 5.0])
                        .show(ui, |ui| {
                            for (key, value) in entries {
                                let name = describe(key).map(|d| d.name).unwrap_or(key);
                                ui.label(name);
                                ui.label(
                                    egui::RichText::new(readable_value(key, value)).color(
                                        egui::Color32::from_rgb(178, 183, 194),
                                    ),
                                );
                                ui.end_row();
                            }
                        });
                    egui::CollapsingHeader::new("Full note")
                        .default_open(false)
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&profile.note)
                                        .small()
                                        .color(egui::Color32::GRAY),
                                )
                                .wrap(),
                            );
                        });
                }
            });
        });
    }

    fn debug(&mut self, ui: &mut egui::Ui) {
        page_header(ui, "Tools", "Hardware, benchmark, installation and screenshots.");
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.spacing_mut().interact_size.y = 28.0;
            card(ui, |ui| {
                ui.strong("This computer");
                ui.horizontal(|ui| {
                    ui.label(if self.hardware.vendor.is_empty() {
                        "Not checked yet".to_string()
                    } else {
                        format!(
                            "{} · {} of graphics memory",
                            self.hardware.vendor.to_uppercase(),
                            vram_label(self.hardware.vram_mb)
                        )
                    });
                    if ui
                        .button("Scan hardware")
                        .on_hover_text("Checks your graphics card and picks the right profile.")
                        .clicked()
                    {
                        self.detect_now();
                        let auto = auto_profile(&self.profiles, &self.hardware);
                        self.load_profile(&auto);
                    }
                });
                egui::CollapsingHeader::new("Details")
                    .default_open(false)
                    .show(ui, |ui| {
                        if self.detect_output.is_empty() {
                            ui.label(egui::RichText::new("Scan to see the details.").small());
                        } else {
                            ui.monospace(&self.detect_output);
                        }
                    });
            });

            ui.add_space(4.0);
            card(ui, |ui| {
                ui.strong("Benchmark");
                ui.add(
                    egui::Label::new(
                        egui::RichText::new("Runs the game for 20 seconds with these settings and reports the frame rate. Takes about two minutes.")
                            .small()
                            .color(egui::Color32::GRAY),
                    )
                    .wrap(),
                );
                ui.horizontal(|ui| {
                    let busy = self.bench_running();
                    let playing = self.running();
                    if ui
                        .add_enabled(!busy && !playing, egui::Button::new("Run benchmark"))
                        .clicked()
                    {
                        self.bench_now();
                    }
                    if busy {
                        ui.label(egui::RichText::new("Running…").small());
                    } else if playing {
                        ui.label(egui::RichText::new("Close the game first.").small().color(egui::Color32::GRAY));
                    }
                });
                if self.bench_progress > 0.0 {
                    ui.add(
                        egui::ProgressBar::new(self.bench_progress)
                            .text(&self.bench_phase)
                            .animate(self.bench_running()),
                    );
                }
                if !self.bench_result.is_empty() {
                    ui.label(
                        egui::RichText::new(&self.bench_result)
                            .small()
                            .color(egui::Color32::from_rgb(87, 199, 133)),
                    );
                }
            });

            ui.add_space(4.0);
            self.runtime_card(ui);

            ui.add_space(4.0);
            ui.add_space(4.0);
            card(ui, |ui| {
                ui.strong("Screenshots");
                let mut save = !self.settings.capture_dir.trim().is_empty();
                if ui.checkbox(&mut save, "Take screenshots").changed() {
                    if save {
                        if self.settings.capture_dir.trim().is_empty() {
                            self.settings.capture_dir = home_dir()
                                .map(|h| h.join("Pictures/nullifier").display().to_string())
                                .unwrap_or_default();
                        }
                    } else {
                        self.settings.capture_dir.clear();
                    }
                }
                if save {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new("A still picture at each frame number below — one PNG each, not a video.")
                                .small()
                                .color(egui::Color32::GRAY),
                        )
                        .wrap(),
                    );
                    ui.horizontal(|ui| {
                        ui.label("Folder");
                        ui.text_edit_singleline(&mut self.settings.capture_dir);
                        if ui.button("Choose…").clicked() {
                            self.pick_for = Pick::Capture;
                            self.pick_dir = PathBuf::from(&self.settings.capture_dir);
                            if !self.pick_dir.is_dir() {
                                self.pick_dir = home_dir().unwrap_or_else(|| PathBuf::from("/"));
                            }
                            self.picking = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Frame numbers");
                        ui.add_sized(
                            [140.0, 22.0],
                            egui::TextEdit::singleline(&mut self.settings.capture_frames),
                        )
                        .on_hover_text("Counted from the first frame the game draws. 1024 is about a minute in.");
                    });
                }
            });

            ui.add_space(4.0);
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.key_pressed(egui::Key::Escape))
            && self.tab != Tab::Play
            && self.info.is_none()
            && !self.picking
            && self.community_open.is_none()
        {
            self.tab = Tab::Play;
        }
        if let Some(rx) = &self.community_rx {
            if let Ok(board) = rx.try_recv() {
                self.community_rx = None;
                self.community_fetching = false;
                match board {
                    Some(mut board) => {
                        for entry in &mut board {
                            entry.remote = true;
                        }
                        normalize_community(&mut board);
                        let shared = board.len();
                        self.community.retain(|entry| !entry.remote);
                        self.community.extend(board);
                        let _ = shared;
                        self.community_status.clear();
                    }
                    None => {
                        self.community_status =
                            "Could not reach the published board (offline?).".into();
                    }
                }
                self.save_community_after_fetch();
            }
        }
        if let Some(rx) = &self.update_rx {
            if let Ok(tag) = rx.try_recv() {
                self.update_tag = tag;
                self.update_rx = None;
            }
        }
        self.tick_bench();
        self.tick_prepare(ctx);
        self.refresh_log();
        if let Some((_, at)) = &self.toast {
            if at.elapsed() > Duration::from_secs(6) {
                self.toast = None;
            } else {
                ctx.request_repaint_after(Duration::from_millis(500));
            }
        }
        if self.running() || self.bench_running() || self.prep.is_some() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }

        // Drop a folder (or the .exe) anywhere on the window to pick the game.
        let dropped: Option<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .next()
        });
        if let Some(path) = dropped {
            let dir = if path.is_dir() {
                path
            } else {
                path.parent().map(Path::to_path_buf).unwrap_or(path)
            };
            self.settings.game_dir = dir.display().to_string();
            self.settings.save();
            self.status = match title_exe(&dir) {
                Some(exe) => format!("found {}", exe.file_name().and_then(|n| n.to_str()).unwrap_or("the game")),
                None => "that folder has no .exe in it".into(),
            };
            self.toast = Some((self.status.clone(), std::time::Instant::now()));
        }

        egui::SidePanel::left("rail")
            .frame(egui::Frame::new().fill(egui::Color32::from_rgb(27, 30, 38)).inner_margin(egui::Margin::symmetric(14, 14)).stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(42, 46, 56))))
            .exact_width(190.0)
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_space(12.0);
                ui.label(egui::RichText::new("Nullifier").size(19.0).strong());
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("launcher {VERSION}"))
                            .size(11.0)
                            .color(egui::Color32::GRAY),
                    );
                    if let Some(tag) = self.update_tag.clone() {
                        if is_newer(&tag, VERSION) {
                            let dot = ui
                                .add(
                                    egui::Label::new(
                                        egui::RichText::new("● update")
                                            .size(10.0)
                                            .color(ACCENT),
                                    )
                                    .sense(egui::Sense::click()),
                                )
                                .on_hover_text(format!(
                                    "{tag} is available - click to open the releases page"
                                ))
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                            if dot.clicked() {
                                open_url(RELEASES_URL);
                            }
                        }
                    }
                });
                ui.label(
                    egui::RichText::new(format!("layer v{LAYER_VERSION}"))
                        .size(10.0)
                        .color(egui::Color32::from_rgb(120, 126, 138)),
                );
                ui.add_space(18.0);
                if !self.settings.show_advanced
                    && !matches!(
                        self.tab,
                        Tab::Play | Tab::Graphics | Tab::Controls | Tab::Community
                    )
                {
                    self.tab = Tab::Play;
                }
                nav_group(ui, "GAME");
                for (tab, label) in [
                    (Tab::Play, "Play"),
                    (Tab::Graphics, "Graphics"),
                    (Tab::Controls, "Controls"),
                    (Tab::Community, "Community"),
                ] {
                    if nav_item(ui, label, self.tab == tab).clicked() {
                        self.tab = tab;
                    }
                }
                if self.settings.show_advanced {
                    nav_group(ui, "DEVELOPER");
                    for (tab, label) in [
                        (Tab::Rendering, "Rendering"),
                        (Tab::Advanced, "Advanced"),
                        (Tab::Debug, "Tools"),
                    ] {
                        if nav_item(ui, label, self.tab == tab).clicked() {
                            self.tab = tab;
                        }
                    }
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    ui.add_space(10.0);
                    let label = if self.settings.show_advanced {
                        "Hide advanced"
                    } else {
                        "Advanced settings"
                    };
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(label)
                                    .size(11.0)
                                    .color(egui::Color32::from_rgb(140, 146, 158)),
                            )
                            .fill(egui::Color32::TRANSPARENT)
                            .stroke(egui::Stroke::NONE),
                        )
                        .clicked()
                    {
                        self.settings.show_advanced = !self.settings.show_advanced;
                        self.settings.save();
                    }
                    ui.add_space(4.0);
                });
            });

        let footer = egui::TopBottomPanel::bottom("log")
            .frame(egui::Frame::new().fill(egui::Color32::from_rgb(27, 30, 38)).inner_margin(egui::Margin::symmetric(16, 8)).stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(42, 46, 56))))
            .resizable(false);
        let footer = if self.log_open { footer.exact_height(180.0) } else { footer };
        footer
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let live = self.running() || self.bench_running();
                    ui.label(
                        egui::RichText::new(capitalise(&self.status))
                            .small()
                            .color(if live { egui::Color32::from_rgb(87, 199, 133) } else { egui::Color32::GRAY }),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(10.0);
                        ui.style_mut().spacing.button_padding = egui::vec2(9.0, 4.0);
                        ui.style_mut().spacing.interact_size.y = 26.0;
                        ui.style_mut().spacing.item_spacing.x = 6.0;
                        ui.style_mut().visuals.widgets.noninteractive.corner_radius = 4.into();
                        ui.style_mut().visuals.widgets.inactive.corner_radius = 4.into();
                        ui.style_mut().visuals.widgets.hovered.corner_radius = 4.into();
                        ui.style_mut().visuals.widgets.active.corner_radius = 4.into();
                        if chip_button(ui, if self.log_open { "Hide log" } else { "Show log" }).clicked() {
                            self.log_open = !self.log_open;
                        }
                        if self.log_open {
                            if chip_button(ui, "Clear").clicked() {
                                self.log_shown.clear();
                            }
                            if chip_button(ui, "Open log").clicked() {
                                if let Some(path) = &self.log_path {
                                    open_path(path);
                                }
                            }
                            let shown = match self.settings.log_level.as_str() {
                                "warn" => "Quiet",
                                "debug" => "Detailed",
                                "trace" => "Everything",
                                _ => "Normal",
                            };
                            egui::ComboBox::from_id_salt("log detail footer")
                                .selected_text(egui::RichText::new(shown).small())
                                .width(92.0)
                                .show_ui(ui, |ui| {
                                    for (value, label) in [
                                        ("warn", "Quiet"),
                                        ("info", "Normal"),
                                        ("debug", "Detailed"),
                                        ("trace", "Everything"),
                                    ] {
                                        ui.selectable_value(&mut self.settings.log_level, value.to_string(), label);
                                    }
                                });
                            ui.label(egui::RichText::new("Log detail").small().color(egui::Color32::GRAY));
                        }
                    });
                });
                if self.log_open {
                    egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                        ui.monospace(&self.log_shown);
                    });
                }
                ui.add_space(4.0);
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(egui::Color32::from_rgb(23, 25, 31)).inner_margin(egui::Margin::symmetric(22, 18)))
            .show(ctx, |ui| match self.tab {
            Tab::Play => self.play(ui),
            Tab::Graphics => self.game_page(ui),
            Tab::Controls => self.controls(ui),
            Tab::Community => self.community_page(ui),
            Tab::Rendering => self.rendering(ui),
            Tab::Advanced => self.advanced(ui),
            Tab::Debug => self.debug(ui),
        });

        if let Some(key) = self.info.clone() {
            let mut open = true;
            let desc = describe(&key);
            egui::Window::new(desc.as_ref().map(|d| d.name).unwrap_or("This switch"))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(420.0)
                .show(ctx, |ui| {
                    ui.label(egui::RichText::new(&key).monospace().small().color(egui::Color32::GRAY));
                    ui.add_space(4.0);
                    match &desc {
                        Some(d) => {
                            ui.label(d.plain);
                            ui.add_space(6.0);
                            ui.label(egui::RichText::new(format!("Safe: {}", d.safe)).small().color(egui::Color32::from_rgb(87, 199, 133)));
                        }
                        None => {
                            ui.label("This build does not document this switch. It is likely ignored, or reserved for a future version.");
                            ui.label(egui::RichText::new("Check docs/FINDINGS.md or the source before relying on it.").small().color(egui::Color32::GRAY));
                        }
                    }
                });
            if !open {
                self.info = None;
            }
        }

        if self.picking {
            let mut open = true;
            egui::Window::new("Choose the game folder")
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .default_size([560.0, 380.0])
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Up").clicked() {
                            if let Some(parent) = self.pick_dir.parent() {
                                self.pick_dir = parent.to_path_buf();
                            }
                        }
                        ui.monospace(self.pick_dir.display().to_string());
                    });
                    ui.separator();
                    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                        let mut entries: Vec<PathBuf> = std::fs::read_dir(&self.pick_dir)
                            .into_iter()
                            .flatten()
                            .flatten()
                            .map(|e| e.path())
                            .filter(|p| p.is_dir())
                            .collect();
                        entries.sort();
                        for dir in entries {
                            let name = dir
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("")
                                .to_string();
                            let has_game = dir.join("game_scarlett_master.exe").is_file();
                            let label = if has_game {
                                format!("📁 {name}   ·   game found")
                            } else {
                                format!("📁 {name}")
                            };
                            let text = if has_game {
                                egui::RichText::new(label).color(egui::Color32::from_rgb(160, 214, 180))
                            } else {
                                egui::RichText::new(label)
                            };
                            if ui.selectable_label(false, text).clicked() {
                                self.pick_dir = dir;
                            }
                        }
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.add(primary("Use this folder", 140.0, 32.0)).clicked()
                        {
                            let picked = self.pick_dir.display().to_string();
                            match self.pick_for {
                                Pick::Game => {
                                    self.settings.game_dir = picked;
                                    self.settings.save();
                                    self.tab = Tab::Play;
                                    self.status = match title_exe(&self.pick_dir) {
                                        Some(exe) => format!(
                                            "found {}",
                                            exe.file_name().and_then(|n| n.to_str()).unwrap_or("the game")
                                        ),
                                        None => "game_scarlett_master.exe is not in that folder".into(),
                                    };
                                    if title_exe(&self.pick_dir).is_some() {
                                        self.toast = Some((self.status.clone(), std::time::Instant::now()));
                                    }
                                }
                                Pick::Capture => self.settings.capture_dir = picked,
                            }
                            self.picking = false;
                        }
                        if ui.button("Cancel").clicked() {
                            self.picking = false;
                        }
                    });
                });
            if !open {
                self.picking = false;
            }
        }
    }
}

// ---------------------------------------------------------------- words

struct Desc {
    name: &'static str,
    plain: &'static str,
    safe: &'static str,
}

/// What a sensible value looks like, for fields that take a bare number.
fn example(key: &str) -> Option<&'static str> {
    match key {
        "VICTORY_TEXTURE_BUDGET_MB" => Some("2048, 3072 or 4096 (MB)"),
        "VICTORY_CARD_BUFFERS_MB" => Some("256, 512 or 1024 (MB)"),
        "VICTORY_CARD_BUFFERS_MIN_MB" => Some("1 (MB)"),
        "VICTORY_LATE_WAIT" => Some("0 to 3"),
        "VICTORY_CARD_REHASH_RUNS" => Some("0, 1 or 2"),
        "VICTORY_INDIRECT_RUNS" | "VICTORY_INDIRECT_WINDOW" | "VICTORY_RELAY_QUEUE" => Some("1"),
        "VICTORY_BACK_OWN_VIEW" | "VICTORY_ALIAS_LAZY" => Some("1"),
        "VICTORY_RELAY" => Some("0, 1 or 2"),
        "VICTORY_FPS_CAP" => Some("30, 60, 120 or 0 for unlimited"),
        "VICTORY_VSYNC" => Some("1 = on"),
        "gamma" => Some("1-50; 15 is the default"),
        "graphicsQualityPreset" => {
            Some("1 low, 2 medium, 3 high, 4 ultra; 0 is custom")
        }
        "hdrIntensity" => Some("0-100; 50 is the default"),
        "hdrBlackLevels" => Some("0-100; 50 is the default"),
        "hdrPeakBrightness" => Some("600-1600 nits"),
        "anisotropicFiltering" => Some("0, 4, 8 or 16"),
        "frameRateLimit" => Some("30, 60, 120, 144, 240 or 0 for unlimited"),
        "desiredFPS" => Some("30, 60 or 120"),
        "vSync" => Some("0 = off, 1 = on, 2 = adaptive"),
        "fsrQuality" => Some("0 = off, 1 = quality, 2 = balanced, 3 = performance"),
        "fsrSharpen" => Some("0.0 to 1.0 (0.2 is the default)"),
        "sharpenIntensity" => Some("0.0 to 2.0 (1.0 is the default)"),
        "lodScale" | "grassLod" => Some("1.0 is the default; lower is faster"),
        "texLodScale_Global" => Some("1.0 is the default"),
        "texLodScale_LOD" => Some("2.0 is the default"),
        "texLodScale_SLOD1" => Some("4.0 is the default"),
        "texLodScale_SLOD2" => Some("8.0 is the default"),
        "texLodScale_SLOD3" => Some("16.0 is the default"),
        "texLodScale_SLOD4" => Some("32.0 is the default"),
        "texLodScale_Mat_High" => Some("0.25 is the default"),
        "texLodScale_Mat_Med" => Some("0.5 is the default"),
        "texLodScale_Mat_Low" => Some("1.0 is the default"),
        "texLodScale_Buildings" | "texLodScale_Objects" | "texLodScale_HD"
        | "texLodScale_OrphanHD" => Some("1.0 is the default"),
        "vehicleLodBias" => Some("0.0 is the default"),
        "manualMipBias" => Some("0.0 is the default; negative is sharper"),
        "waterReflectionResX" => Some("128, 256, 512 or 1024"),
        "waterReflectionResY" => Some("64, 128, 256 or 512"),
        "width" => Some("1920, 2560 or 3840"),
        "height" => Some("1080, 1440 or 2160"),
        "motionBlurLimit" => Some("8, 16 or 32"),
        "motionBlurScale" => Some("0.5 to 2.0 (1.0 is the default)"),
        "maxTexUpgradesPerFrame" => Some("5 is the default"),
        "ssroSampleCount" => Some("2 or 4"),
        "vehicleHDCount" => Some("1, 2 or 4"),
        "lensFlaresScreenResDivExponent" => Some("2 is the default"),
        _ => None,
    }
}

const KNOWN_SWITCHES: &[(&str, &str)] = &[
    ("VICTORY_VSYNC", "0"),
    ("VICTORY_FPS_CAP", "0"),
    ("VICTORY_TEXTURE_BUDGET_MB", "4096"),
    ("VICTORY_CARD_BUFFERS_MB", "1024"),
    ("VICTORY_CARD_BUFFERS_MIN_MB", "1"),
    ("VICTORY_TEXTURE_NO_WAIT", "1"),
    ("VICTORY_CARD_HASH_LANES", "1"),
    ("VICTORY_WIDE_BACK", "1"),
    ("VICTORY_BACK_OWN_VIEW", "1"),
    ("VICTORY_ALIAS_LAZY", "1"),
    ("VICTORY_CARD_REHASH_RUNS", "1"),
    ("VICTORY_INDIRECT_WINDOW", "1"),
    ("VICTORY_INDIRECT_RUNS", "1"),
    ("VICTORY_JOIN_COPIES", "1"),
    ("VICTORY_RELAY", "2"),
    ("VICTORY_RELAY_QUEUE", "1"),
    ("VICTORY_LATE_WAIT", "1"),
    ("VICTORY_LOG", "info"),
];

fn danger_remove(ui: &mut egui::Ui) -> egui::Response {
    ui.add(
        egui::Button::new(
            egui::RichText::new("Remove")
                .size(11.0)
                .color(egui::Color32::from_rgb(240, 126, 126)),
        )
        .fill(egui::Color32::from_rgb(43, 27, 29))
        .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(126, 62, 62)))
        .min_size(egui::vec2(70.0, 28.0))
        .corner_radius(6),
    )
    .on_hover_text("Removing a setting turns it off")
}

fn describe(key: &str) -> Option<Desc> {
    let d = |name, plain, safe| Some(Desc { name, plain, safe });
    match key {
        "VICTORY_TEXTURE_BUDGET_MB" => d(
            "Texture memory budget",
            "Texture data kept on the card. Higher looks sharper in motion; lower leaves room for other apps.",
            "2048–4096 MB on a 16 GB card. Never more than your card's VRAM.",
        ),
        "VICTORY_CARD_BUFFERS_MB" => d(
            "World memory kept on the card",
            "The game's working memory mirrored on the card. Too much competes with textures.",
            "512 MB measured best on 16 GB; 256 MB on an 8 GB card.",
        ),
        "VICTORY_CARD_BUFFERS_MIN_MB" => d(
            "Smallest buffer kept on the card",
            "Buffers at least this large are mirrored in card memory.",
            "1 MB. Leave it.",
        ),
        "VICTORY_LATE_WAIT" => d(
            "Frames prepared ahead",
            "Frames handed to the card ahead of the last. 2 and above can crash on Linux with an AMD card.",
            "0 or 1 with an AMD card on Linux. 3 is measured with a 16 GB NVIDIA card on Linux (linux-nvidia-16gb), and is what the Windows profiles set.",
        ),
        "VICTORY_TEXTURE_NO_WAIT" => d(
            "Do not wait for textures",
            "Show a frame without waiting for a texture upload. Faster; textures can pop in late.",
            "On. Turn it off only if textures look wrong.",
        ),
        "VICTORY_CARD_HASH_LANES" => d(
            "Faster card checks",
            "Check card copies with a team of threads. Without it the check is the largest thing on the card.",
            "On. 4.45 -> 6.6 fps here, no crashes.",
        ),
        "VICTORY_WIDE_BACK" => d(
            "Copy results back in whole ranges",
            "Copy shader results back in whole ranges instead of small pieces.",
            "On.",
        ),
        "VICTORY_BACK_OWN_VIEW" => d(
            "Copy results back through a view of our own",
            "Shader results go back to the title's memory through a view of the layer's own, so they are not sent up to the card again.",
            "On with a 16 GB NVIDIA card on Linux (linux-nvidia-16gb). Not measured elsewhere.",
        ),
        "VICTORY_ALIAS_LAZY" => d(
            "Guard only the views that are written",
            "A write through one view of a guarded page leaves the page's other views read-only until they are written too.",
            "On with a 16 GB NVIDIA card on Linux (linux-nvidia-16gb). Not measured elsewhere.",
        ),
        "VICTORY_CARD_REHASH_RUNS" => d(
            "Re-check only changed ranges",
            "Only re-check card copies that changed.",
            "On is safe; the effect is small.",
        ),
        "VICTORY_INDIRECT_WINDOW" => d(
            "Batch indirect draws",
            "Handle many draw records in windows. Measured about 25% faster here.",
            "On.",
        ),
        "VICTORY_INDIRECT_RUNS" => d(
            "Batch single draw records",
            "One pass per run of single draw records. Measured about 18% faster here.",
            "On.",
        ),
        "VICTORY_JOIN_COPIES" => d(
            "Join neighbouring copies",
            "Send adjacent copies as one. Has crashed the driver before.",
            "Off unless you are testing.",
        ),
        "VICTORY_RELAY" => d(
            "Hand results back beside writes",
            "Hand results back beside writes. 2 is faster but has crashed before.",
            "0, or 2 if your machine stays stable with it.",
        ),
        "VICTORY_RELAY_QUEUE" => d(
            "A queue of its own for handed-back results",
            "With Relay on, results are fetched on a copy queue of their own, so they do not wait behind the next frame's work.",
            "On with Relay 2 on a 16 GB NVIDIA card on Linux (linux-nvidia-16gb). It does nothing with Relay 0.",
        ),
        "VICTORY_VSYNC" => d(
            "V-Sync",
            "Wait for the display. Removes tearing, adds a little delay.",
            "Off unless you see tearing; it matters most above 30 fps.",
        ),
        "VICTORY_FPS_CAP" => d(
            "Frame rate limit",
            "Hold the game under this rate. 0 is unlimited; lower limits steady heat and noise.",
            "0 (off), 60, 120 or 144. Below 30 the picture feels heavy.",
        ),
        "VICTORY_LOG" => d(
            "Log detail",
            "How much is written to the log. More detail is slower.",
            "info for play; warn if you want quiet.",
        ),
        _ => None,
    }
}

fn setting_value(sets: &BTreeMap<String, String>, key: &str) -> Option<u32> {
    sets.get(key).and_then(|v| v.trim().parse().ok())
}

/// Values that cannot do what the player expects on this machine.
fn warnings(settings: &Settings, hw: &Hardware) -> Vec<String> {
    let sets = &settings.sets;
    let vram = hw.vram_mb;
    let mut out = Vec::new();
    if let Some(budget) = setting_value(sets, "VICTORY_TEXTURE_BUDGET_MB") {
        if budget == 0 {
            out.push("Texture budget is 0: no texture can be copied, the world will stay untextured.".into());
        } else if vram > 0 && budget > vram {
            out.push(format!(
                "Texture budget is {budget} MB, more than your card's {vram} MB. It spills into system memory and can stall or fail."
            ));
        } else if budget < 1024 {
            out.push("Texture budget below 1024 MB: textures may arrive late or stay black. 2048 MB or more is safer.".into());
        }
    }
    if let Some(buffers) = setting_value(sets, "VICTORY_CARD_BUFFERS_MB") {
        if vram > 0 && buffers > vram / 2 {
            out.push(format!(
                "World memory on the card is {buffers} MB, over half of your {vram} MB. Textures will have little room left."
            ));
        }
    }
    if let Some(late) = setting_value(sets, "VICTORY_LATE_WAIT") {
        // On Linux with an AMD card (RADV) 2 and 3 remove the device within
        // minutes. With an NVIDIA card 3 is the measured value of the
        // linux-nvidia-16gb profile (docs/FORK.md), and the Windows
        // profiles set 3.
        if late >= 2 && !cfg!(windows) && hw.vendor != "nvidia" {
            out.push("Frames prepared ahead is 2 or more: this is known to remove the device on Linux with an AMD card. Use 0 or 1.".into());
        }
    }
    if sets.contains_key("VICTORY_JOIN_COPIES") {
        out.push("Joined copies can crash the graphics driver; it did before on this project.".into());
    }
    // With an NVIDIA card on Linux, 2 is the measured value of the
    // linux-nvidia-16gb profile, together with VICTORY_RELAY_QUEUE=1
    // (docs/FORK.md).
    if setting_value(sets, "VICTORY_RELAY") == Some(2) && !cfg!(windows) && hw.vendor != "nvidia" {
        out.push("Relay 2 has crashed the driver before on Linux. If a run dies, set it to 0.".into());
    }
    for key in sets.keys() {
        if key.starts_with("VICTORY_") && describe(key).is_none() {
            out.push(format!("{key} is not a switch this build knows; it will be ignored."));
        }
    }
    out
}

// ---------------------------------------------------------------- widgets

fn preset_card(ui: &mut egui::Ui, selected: bool, title: &str, desc: &str) -> egui::Response {
    let fill = if selected { ACCENT } else { egui::Color32::from_rgb(38, 41, 48) };
    let stroke = if selected { ACCENT } else { egui::Color32::from_rgb(52, 56, 64) };
    let frame = egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0_f32, stroke))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(12, 9));
    let response = frame
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                let painter = ui.painter();
                if selected {
                    painter.circle_filled(rect.center(), 6.0, egui::Color32::WHITE);
                    painter.circle_filled(rect.center(), 3.0, ACCENT);
                } else {
                    painter.circle_stroke(rect.center(), 6.0, egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(120, 125, 136)));
                }
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(title)
                            .strong()
                            .color(if selected { egui::Color32::WHITE } else { egui::Color32::from_rgb(225, 228, 236) }),
                    );
                    ui.label(
                        egui::RichText::new(desc).size(11.5).color(if selected {
                            egui::Color32::from_rgb(235, 240, 255)
                        } else {
                            egui::Color32::from_rgb(158, 163, 174)
                        }),
                    );
                });
            });
        })
        .response;
    response.interact(egui::Sense::click())
}

fn pair(ms: Option<f32>, mb: Option<f32>) -> Option<String> {
    match (ms, mb) {
        (Some(ms), Some(mb)) => Some(format!("{ms:.1} ms · {mb:.0} MB")),
        (Some(ms), None) => Some(format!("{ms:.1} ms")),
        _ => None,
    }
}

fn stat(ui: &mut egui::Ui, label: &str, value: Option<String>) {
    ui.vertical(|ui| {
        ui.set_min_width(150.0);
        ui.label(egui::RichText::new(label).small().color(egui::Color32::GRAY));
        ui.label(value.unwrap_or_else(|| "—".into()));
    });
}

fn chip_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(text).small())
            .corner_radius(4)
            .min_size(egui::vec2(0.0, 26.0)),
    )
}

fn help_button(ui: &mut egui::Ui) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::click());
    let color = if response.hovered() {
        egui::Color32::from_rgb(120, 165, 240)
    } else {
        egui::Color32::from_rgb(96, 132, 196)
    };
    ui.painter()
        .circle_stroke(rect.center(), 8.5, egui::Stroke::new(1.2_f32, color));
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "?",
        egui::FontId::proportional(10.5),
        color,
    );
    response.on_hover_cursor(egui::CursorIcon::Help)
}

fn secondary(text: &str, width: f32, height: f32) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text).color(egui::Color32::from_rgb(220, 224, 233)))
        .fill(egui::Color32::from_rgb(43, 48, 59))
        .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(57, 64, 78)))
        .corner_radius(7)
        .min_size(egui::vec2(width, height))
}

fn primary(text: &str, width: f32, height: f32) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text).strong().color(egui::Color32::WHITE))
        .fill(ACCENT)
        .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(105, 145, 245)))
        .corner_radius(7)
        .min_size(egui::vec2(width, height))
}

fn nav_group(ui: &mut egui::Ui, label: &str) {
    ui.add_space(10.0);
    ui.label(
        egui::RichText::new(label)
            .size(9.5)
            .strong()
            .color(egui::Color32::from_rgb(112, 118, 130)),
    );
    ui.add_space(2.0);
}

fn nav_item(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 36.0), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let background = if selected {
            egui::Color32::from_rgb(43, 58, 91)
        } else if response.hovered() {
            egui::Color32::from_rgb(37, 41, 51)
        } else {
            egui::Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, 7.0, background);
        if selected {
            ui.painter().rect_filled(
                egui::Rect::from_min_size(rect.left_top() + egui::vec2(0.0, 7.0), egui::vec2(3.0, rect.height() - 14.0)),
                2.0,
                ACCENT,
            );
        }
        ui.painter().text(
            rect.left_center() + egui::vec2(12.0, 0.0),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(14.0),
            if selected {
                egui::Color32::from_rgb(235, 240, 251)
            } else {
                egui::Color32::from_rgb(178, 184, 197)
            },
        );
    }
    response
}

fn page_header(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.add_space(2.0);
    ui.label(egui::RichText::new(title).size(23.0).strong());
    if !subtitle.is_empty() {
        ui.add(
            egui::Label::new(
                egui::RichText::new(subtitle)
                    .small()
                    .color(egui::Color32::from_rgb(150, 156, 168)),
            )
            .wrap(),
        );
    }
    ui.add_space(6.0);
}

fn card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(30, 33, 41))
        .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(47, 52, 64)))
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(18, 16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

fn warnings_card(ui: &mut egui::Ui, list: &[String]) {
    if list.is_empty() {
        return;
    }
    egui::Frame::group(ui.style())
        .fill(egui::Color32::from_rgb(45, 38, 24))
        .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(120, 95, 45)))
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new("Check these").strong().color(egui::Color32::from_rgb(240, 200, 120)));
            for line in list {
                ui.label(egui::RichText::new(format!("• {line}")).color(egui::Color32::from_rgb(225, 210, 180)));
            }
        });
    ui.add_space(4.0);
}

/// A numeric switch with its plain name, a ? and its safe range.
fn number_info(
    ui: &mut egui::Ui,
    sets: &mut BTreeMap<String, String>,
    key: &str,
    range: std::ops::RangeInclusive<u32>,
    step: u32,
) -> Option<String> {
    let desc = describe(key);
    let suffix = match key {
        "VICTORY_TEXTURE_BUDGET_MB" | "VICTORY_CARD_BUFFERS_MB" => " MB",
        "VICTORY_FPS_CAP" => " fps",
        _ => "",
    };
    let mut value = setting_value(sets, key).unwrap_or(*range.start());
    let mut asked = None;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.horizontal(|ui| {
            ui.label(desc.as_ref().map(|d| d.name).unwrap_or(key));
            if desc.is_some() && help_button(ui).clicked() {
                asked = Some(key.to_string());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_sized(
                        [96.0, 28.0],
                        egui::DragValue::new(&mut value)
                            .range(range.clone())
                            .speed(step as f64)
                            .suffix(suffix),
                    )
                    .changed()
                {
                    sets.insert(key.to_string(), value.to_string());
                }
            });
        });
        if let Some(d) = &desc {
            ui.add(
                egui::Label::new(egui::RichText::new(d.plain).small().color(egui::Color32::GRAY))
                    .wrap(),
            );
            if let Some(eg) = example(key) {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(format!("Examples: {eg}"))
                            .small()
                            .color(egui::Color32::from_rgb(120, 126, 138)),
                    )
                    .wrap(),
                );
            }
        }
    });
    ui.add_space(8.0);
    asked
}

/// A switch with its plain name, a ? and a one-line reason.
fn switch_info(
    ui: &mut egui::Ui,
    sets: &mut BTreeMap<String, String>,
    key: &str,
) -> Option<String> {
    let desc = describe(key);
    let mut on = sets.contains_key(key);
    let mut asked = None;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.horizontal(|ui| {
            if ui.checkbox(&mut on, desc.as_ref().map(|d| d.name).unwrap_or(key)).changed() {
                if on {
                    sets.insert(key.to_string(), "1".into());
                } else {
                    sets.remove(key);
                }
            }
            if desc.is_some() && help_button(ui).clicked() {
                asked = Some(key.to_string());
            }
        });
        if let Some(d) = &desc {
            ui.add(
                egui::Label::new(egui::RichText::new(d.plain).small().color(egui::Color32::GRAY))
                    .wrap(),
            );
        }
    });
    ui.add_space(8.0);
    asked
}

/// The settings a player looks for first, in the order they normally appear in
/// a game's options. Everything else goes under Advanced graphics settings.
const DISPLAY_KEYS: &[(&str, &str)] = &[("advancedGraphics", "motionBlur")];

const GRAPHICS_KEYS: &[(&str, &str)] = &[
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

const SHADOW_KEYS: &[(&str, &str)] = &[
    ("advancedGraphics", "directionalScreenSpaceShadowQuality"),
    ("advancedGraphics", "worldHeightShadowQuality"),
    ("advancedGraphics", "waterReflectionResX"),
    ("advancedGraphics", "waterReflectionResY"),
    ("advancedGraphics", "waterReflectionSSR"),
];

fn is_curated(section: &str, key: &str) -> bool {
    DISPLAY_KEYS
        .iter()
        .chain(GRAPHICS_KEYS)
        .chain(SHADOW_KEYS)
        .any(|(s, k)| *s == section && *k == key)
        || (section == "video/resolutionFullscreen" && (key == "width" || key == "height"))
        || (section == "video" && (key == "vSync" || key == "frameRateLimit"))
}

fn render_index(ui: &mut egui::Ui, game: &mut GameSettings, index: usize) {
    let changed = {
        let setting = &mut game.settings[index];
        game_row(ui, setting)
    };
    if changed {
        let setting = &game.settings[index];
        let line = format!(
            "{}<{} value=\"{}\" />",
            setting.indent, setting.key, setting.value
        );
        game.lines[setting.line] = line;
        game.dirty = true;
    }
}

fn render_pairs(ui: &mut egui::Ui, game: &mut GameSettings, pairs: &[(&str, &str)]) {
    for (section, key) in pairs {
        if let Some(index) = game
            .settings
            .iter()
            .position(|s| s.section == *section && s.key == *key)
        {
            render_index(ui, game, index);
        }
    }
}

fn section_header(ui: &mut egui::Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(egui::RichText::new(title).strong().size(15.0));
    ui.separator();
}

fn apply_game(game: &mut GameSettings) -> std::io::Result<()> {
    let backup = game.path.with_file_name("settings.xml.bak");
    std::fs::copy(&game.path, &backup)?;
    let mut text = game.lines.join("\n");
    text.push('\n');
    std::fs::write(&game.path, text)
}

fn label_of(setting: &GameSetting) -> String {
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
fn float_key(key: &str) -> bool {
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

fn float_range(key: &str) -> (f32, f32, f32) {
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
fn choices(key: &str) -> Option<&'static [(&'static str, &'static str)]> {
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
fn int_slider(key: &str) -> Option<(i64, i64, &'static str)> {
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

fn limits(key: &str) -> Option<(f64, f64, f64)> {
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

/// The title ignores these in the current build; say so next to the value.
fn no_effect(key: &str) -> bool {
    matches!(
        key,
        "motionBlur" | "rtDynamicShadows" | "rtLODLightShadows"
    )
}

fn game_row(ui: &mut egui::Ui, setting: &mut GameSetting) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label_of(setting));
        if let Some(eg) = example(&setting.key) {
            ui.label(
                egui::RichText::new(format!("· {eg}"))
                    .small()
                    .color(egui::Color32::from_rgb(120, 126, 138)),
            );
        }
        if no_effect(&setting.key) {
            ui.label(
                egui::RichText::new("· no effect")
                    .small()
                    .color(egui::Color32::from_rgb(232, 179, 74)),
            )
            .on_hover_text("The title ignores this setting in the current build.");
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(options) = choices(&setting.key) {
                let number = setting.value.parse::<f32>().ok();
                let selected = options.iter().find(|(_, value)| {
                    number.is_some_and(|n| {
                        (n - value.parse::<f32>().unwrap_or(f32::NAN)).abs() < 0.001
                    })
                });
                let current = selected
                    .map(|(label, _)| (*label).to_string())
                    .unwrap_or_else(|| format!("Custom ({})", setting.value));
                let mut chosen: Option<String> = None;
                egui::ComboBox::from_id_salt(format!("choice {}", setting.key))
                    .selected_text(current)
                    .width(150.0)
                    .show_ui(ui, |ui| {
                        for (label, value) in options {
                            let is = selected.is_some_and(|(_, v)| v == value);
                            if ui.selectable_label(is, *label).clicked() && !is {
                                chosen = Some((*value).to_string());
                            }
                        }
                    });
                if let Some(value) = chosen {
                    setting.value = value;
                    changed = true;
                }
            } else if setting.value == "true" || setting.value == "false" {
                let mut on = setting.value == "true";
                if ui.checkbox(&mut on, "").changed() {
                    setting.value = if on { "true".into() } else { "false".into() };
                    changed = true;
                }
            } else if float_key(&setting.key) || setting.value.contains('.') {
                let before = setting.value.parse::<f32>().unwrap_or(0.0);
                let mut v = before;
                let (min, max, step) = float_range(&setting.key);
                let shown = if (min, max) == (0.0, 1.0) {
                    format!("{:.0}%", v * 100.0)
                } else if setting.key.contains("lod") || setting.key.contains("Lod") {
                    format!("{v:.2}×")
                } else {
                    format!("{v:.2}")
                };
                ui.label(egui::RichText::new(shown).small());
                let slider = egui::Slider::new(&mut v, min..=max)
                    .step_by(f64::from(step))
                    .show_value(false);
                ui.add_sized([150.0, 20.0], slider);
                if (v - before).abs() > f32::EPSILON {
                    setting.value = format!("{v:.6}");
                    changed = true;
                }
            } else if let Ok(mut v) = setting.value.parse::<i64>() {
                if let Some((min, max, suffix)) = int_slider(&setting.key) {
                    let before = v;
                    let slider = egui::Slider::new(&mut v, min..=max)
                        .step_by(1.0)
                        .suffix(suffix);
                    ui.add_sized([170.0, 20.0], slider);
                    if v != before {
                        setting.value = v.to_string();
                        changed = true;
                    }
                } else {
                    let before = v;
                    let mut drag = egui::DragValue::new(&mut v);
                    if let Some((min, max, _)) = limits(&setting.key) {
                        drag = drag.range(min as i64..=max as i64);
                    }
                    if ui.add_sized([96.0, 28.0], drag).changed() && v != before {
                        setting.value = v.to_string();
                        changed = true;
                    }
                }
            } else if ui
                .add_sized([180.0, 24.0], egui::TextEdit::singleline(&mut setting.value))
                .changed()
            {
                changed = true;
            }
        });
    });
    changed
}

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn kill_game() {
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

const RELEASES_URL: &str = "https://github.com/iNiKKo/Nullifier/releases/latest";

fn version_parts(text: &str) -> Vec<u32> {
    text.trim()
        .trim_start_matches('v')
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.parse().ok())
        .collect()
}

fn is_newer(latest: &str, current: &str) -> bool {
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
fn update_check() -> (Option<String>, Option<std::sync::mpsc::Receiver<Option<String>>>) {
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

fn open_url(url: &str) {
    open_path(Path::new(url));
}

fn open_path(path: &Path) {
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

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn read_tail(path: &Path, max: usize) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let start = len.saturating_sub(max as u64);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    if start > 0 {
        // The first line is cut: drop it.
        Some(text.split_once('\n').map(|(_, rest)| rest.to_string()).unwrap_or(text))
    } else {
        Some(text)
    }
}

fn tail(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.lines().collect();
    let start = all.len().saturating_sub(lines);
    all[start..].join("\n")
}

fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut escape = false;
    for c in line.chars() {
        if escape {
            if c == 'm' {
                escape = false;
            }
        } else if c == '\u{1b}' {
            escape = true;
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod community_tests {
    use super::*;

    #[test]
    fn a_shared_log_has_no_home_paths_addresses_or_mail() {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/tester".into());
        let mut settings = Settings::default();
        settings.game_dir = "/games/title".into();
        let raw = format!(
            "6.7 fps, 150.0 ms/frame at {home}/secret/file and /games/title/x\n\
             contact bob@example.com from 192.168.1.42 and 10.0.0.7\n"
        );
        let out = sanitize_log(&raw, &settings);
        assert!(!out.contains(&home), "{out}");
        assert!(!out.contains("/games/title"), "{out}");
        assert!(!out.contains("bob@example.com"), "{out}");
        assert!(!out.contains("192.168.1.42"), "{out}");
        assert!(out.contains("<ip>"), "{out}");
        assert!(out.contains("6.7 fps"), "{out}");
    }

    #[test]
    fn shared_stats_take_world_frames_only() {
        let raw = "\
30.0 fps, 33.3 ms/frame (0.1 preparing: copies 0.1 ms 0.1 MB; 0.2 on the GPU; 0.0 copying back), 2 draws/frame\n\
INFO victory_gpu::host: 6.6 fps, 151.0 ms/frame (60.0 preparing: copies 50.0 ms 200.0 MB; 30.0 on the GPU; 0.0 copying back), 2904 draws/frame\n\
INFO victory_gpu::host: 4.4 fps, 227.0 ms/frame (90.0 preparing: copies 70.0 ms 245.0 MB; 35.0 on the GPU; 0.0 copying back), 3000 draws/frame\n";
        let (fps, frame, gpu, draws) = world_stats(raw);
        assert!((fps.unwrap() - 5.5).abs() < 0.01, "{fps:?}");
        assert_eq!(frame, Some(227.0));
        assert_eq!(gpu, Some(35.0));
        assert_eq!(draws, Some(3000));
    }

    #[test]
    fn menus_are_not_in_game_frames() {
        let raw = "INFO victory_gpu::host: 30.0 fps, 33.3 ms/frame (0.1 preparing; 0.2 on the GPU), 2 draws/frame\n";
        assert_eq!(world_stats(raw).0, None);
        let mixed = "INFO victory_gpu::host: 30.0 fps, 33.3 ms/frame, 2 draws/frame\n                     INFO victory_gpu::host: 4.4 fps, 227.0 ms/frame, 3000 draws/frame\n";
        assert_eq!(world_stats(mixed).0, Some(4.4));
    }

    #[test]
    fn dates_are_civil() {
        assert_eq!(date_of(0), "1970-01-01");
        assert_eq!(date_of(1_700_000_000), "2023-11-14");
    }
}
