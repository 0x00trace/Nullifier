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
const ACCENT: egui::Color32 = egui::Color32::from_rgb(91, 140, 255);
const VERSION: &str = env!("CARGO_PKG_VERSION");


fn apply_theme(ctx: &egui::Context) {
    use egui::{Color32, FontFamily, FontId, Stroke, TextStyle};
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.spacing.interact_size.y = 28.0;
    style.text_styles = [
        (TextStyle::Heading, FontId::new(20.0, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Small, FontId::new(11.5, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(12.5, FontFamily::Monospace)),
    ]
    .into();
    let mut v = egui::Visuals::dark();
    let panel = Color32::from_rgb(24, 26, 30);
    let card = Color32::from_rgb(31, 33, 39);
    let line = Color32::from_rgb(50, 53, 61);
    let ink = Color32::from_rgb(233, 235, 241);
    v.panel_fill = Color32::from_rgb(19, 20, 23);
    v.window_fill = panel;
    v.faint_bg_color = card;
    v.extreme_bg_color = Color32::from_rgb(15, 16, 19);
    v.override_text_color = Some(ink);
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = 7.into();
    }
    v.widgets.noninteractive.bg_fill = card;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, line);
    v.widgets.inactive.bg_fill = Color32::from_rgb(38, 41, 48);
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(38, 41, 48);
    v.widgets.hovered.bg_fill = Color32::from_rgb(45, 49, 58);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(45, 49, 58);
    v.widgets.active.bg_fill = ACCENT;
    v.widgets.open.bg_fill = Color32::from_rgb(45, 49, 58);
    v.window_corner_radius = 10.into();
    v.menu_corner_radius = 8.into();
    style.visuals = v;
    ctx.set_style(style);
}

fn main() -> eframe::Result<()> {
    let root = find_root();
    let mut settings = Settings::load(root.as_deref());

    if std::env::args().any(|a| a == "--check") {
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
                Some("graphics") => Tab::Graphics,
                Some("advanced") => Tab::Advanced,
                Some("debug") => Tab::Debug,
                _ => Tab::Play,
            };
            app.info = start_info;
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
    #[serde(default)]
    planned: Planned,
    #[serde(skip)]
    root: Option<PathBuf>,
}

/// Toggles that need engine work; kept so the choice survives, never applied.
#[derive(Clone, Serialize, Deserialize)]
struct Planned {
    fsr: bool,
    ray_tracing: bool,
    render_scale: f32,
    vsync: bool,
    fps_cap: u32,
    lod: f32,
}

impl Default for Planned {
    fn default() -> Self {
        Self {
            fsr: false,
            ray_tracing: false,
            render_scale: 1.0,
            vsync: false,
            fps_cap: 0,
            lod: 1.0,
        }
    }
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
            planned: Planned::default(),
            root: None,
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

#[derive(Default, Clone)]
struct Hardware {
    vendor: String,
    vram_mb: u32,
    profile: String,
    raw: String,
}

fn hardware(root: &Path) -> Hardware {
    let raw = detect(root);
    let mut hw = Hardware { raw: raw.clone(), ..Default::default() };
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
            "match": { "os": os_name(), "vendor": "any" },
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
    for (k, v) in extra_env(settings) {
        cmd.env(k, v);
    }
    cmd
}

fn bench_command(root: &Path, settings: &Settings, profile_file: &Path) -> Command {
    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("powershell");
        c.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(root.join("tools/bench_scene.ps1"));
        c
    } else {
        let mut c = Command::new("sh");
        c.arg(root.join("tools/bench_scene.sh")).arg("launcher");
        c
    };
    cmd.env("VICTORY_PROFILES", profile_file);
    cmd.env("VICTORY_WINDOW", "20");
    cmd.env("VICTORY_WARMUP", "8");
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
    status: String,
    picking: bool,
    pick_dir: PathBuf,
    new_key: String,
    new_value: String,
    info: Option<String>,
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

#[derive(PartialEq, Eq, Clone, Copy)]
enum Tab {
    Play,
    Graphics,
    Advanced,
    Debug,
}

#[derive(Default)]
struct LastRun {
    fps: String,
    frame: String,
    phases: String,
    removals: u32,
    heap: String,
}

impl App {
    fn new(settings: Settings) -> Self {
        let profiles = settings.root.as_deref().map(profiles).unwrap_or_default();
        let hardware = settings
            .root
            .as_deref()
            .map(hardware)
            .unwrap_or_default();
        Self {
            settings,
            profiles,
            hardware,
            tab: Tab::Play,
            detect_output: String::new(),
            child: None,
            bench: None,
            log_path: None,
            log_shown: String::new(),
            status: "ready".into(),
            picking: false,
            pick_dir: home_dir().unwrap_or_else(|| PathBuf::from(".")),
            new_key: String::new(),
            new_value: String::new(),
            info: None,
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
        let Some(root) = self.settings.root.clone() else { return };
        if cfg!(windows) && !root.join("tools/bench_scene.ps1").exists() {
            self.status = "the benchmark is Linux-only in this build".into();
            return;
        }
        let Some(profile) = custom_profile(&self.settings) else { return };
        let log = cache_dir().map(|d| d.join("bench-last.log"));
        let mut cmd = bench_command(&root, &self.settings, &profile);
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
                self.status = "benchmarking (about 2 minutes)".into();
            }
            Err(e) => self.status = format!("cannot benchmark: {e}"),
        }
    }

    fn stop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
        }
        if let Some(child) = self.bench.as_mut() {
            let _ = child.kill();
        }
        self.child = None;
        self.bench = None;
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
        self.status = "stopped".into();
    }

    fn refresh_log(&mut self) {
        let Some(path) = &self.log_path else { return };
        if let Ok(text) = std::fs::read_to_string(path) {
            self.log_shown = tail(&text, 160);
        }
    }

    fn last_run(&self) -> LastRun {
        let mut run = LastRun::default();
        let Some(path) = &self.log_path else { return run };
        let Ok(text) = std::fs::read_to_string(path) else { return run };
        run.removals = text.matches("device removed").count() as u32;
        for line in text.lines() {
            let clean = strip_ansi(line);
            if let Some(rest) = clean.split("host: ").nth(1) {
                if let Some((fps, tail)) = rest.split_once(" fps, ") {
                    run.fps = fps.trim().to_string();
                    if let Some((ms, _)) = tail.split_once(" ms/frame") {
                        run.frame = ms.trim().to_string();
                    }
                    if let Some(phases) = rest.split("preparing: ").nth(1) {
                        let short: Vec<&str> = phases
                            .split("; ")
                            .filter(|p| p.starts_with("copies") || p.starts_with("textures"))
                            .collect();
                        run.phases = short.join(" · ");
                    }
                }
                if let Some(heap) = rest.split("heap buffers ").nth(1) {
                    if let Some(passes) = heap.split("passes ").nth(1) {
                        run.heap = passes.split(" ms").next().unwrap_or("").to_string();
                    }
                }
            }
        }
        run
    }

    fn load_profile(&mut self, name: &str) {
        if let Some(profile) = self.profiles.iter().find(|p| p.name == name) {
            self.settings.profile = name.to_string();
            self.settings.sets = profile.sets.clone();
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
        ui.heading("Play");
        let warn = warnings(&self.settings, &self.hardware);
        warnings_card(ui, &warn);
        ui.add_space(4.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            card(ui, |ui| {
                ui.strong("Game");
                ui.horizontal(|ui| {
                    let width = (ui.available_width() - 110.0).max(120.0);
                    ui.add_sized(
                        [width, 28.0],
                        egui::TextEdit::singleline(&mut self.settings.game_dir)
                            .hint_text("the folder holding the title's files"),
                    );
                    if ui.button("Browse…").clicked() {
                        self.pick_dir = PathBuf::from(&self.settings.game_dir);
                        if !self.pick_dir.is_dir() {
                            self.pick_dir = home_dir().unwrap_or_else(|| PathBuf::from("/"));
                        }
                        self.picking = true;
                    }
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Profile");
                    let auto = auto_profile(&self.profiles, &self.hardware);
                    let current = if self.settings.profile == auto || self.settings.profile.is_empty() {
                        format!("Auto ({})", auto)
                    } else {
                        self.settings.profile.clone()
                    };
                    let mut picked = None;
                    egui::ComboBox::from_id_salt("profile")
                        .selected_text(current)
                        .show_ui(ui, |ui| {
                            for profile in &self.profiles {
                                if ui
                                    .selectable_label(self.settings.profile == profile.name, &profile.name)
                                    .on_hover_text(if profile.note.len() > 400 {
                                        &profile.note[..400]
                                    } else {
                                        &profile.note
                                    })
                                    .clicked()
                                {
                                    picked = Some(profile.name.clone());
                                }
                            }
                        });
                    if let Some(name) = picked {
                        self.load_profile(&name);
                    }
                    if ui.button("Detect").clicked() {
                        self.detect_now();
                        let auto = auto_profile(&self.profiles, &self.hardware);
                        self.load_profile(&auto);
                    }
                });
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(if self.hardware.vendor.is_empty() {
                            "hardware not detected".to_string()
                        } else {
                            format!(
                                "{} · {} MB VRAM",
                                self.hardware.vendor.to_uppercase(),
                                self.hardware.vram_mb
                            )
                        })
                        .small()
                        .color(egui::Color32::GRAY),
                    ),
                );
                ui.add_space(8.0);
                let big = if self.running() {
                    egui::Button::new(egui::RichText::new("Stop").strong().color(egui::Color32::WHITE))
                        .fill(egui::Color32::from_rgb(190, 70, 70))
                        .min_size(egui::vec2(ui.available_width(), 46.0))
                } else {
                    primary("Launch", ui.available_width(), 46.0)
                };
                if ui.add(big).clicked() {
                    if self.running() {
                        self.stop();
                    } else {
                        self.launch();
                    }
                }
            });

            ui.add_space(6.0);
            ui.columns(2, |cols| {
                self.quick(&mut cols[0]);
                self.last_run_card(&mut cols[1]);
            });
            ui.add_space(6.0);
            self.runtime_card(ui);
        });
    }

    fn runtime_card(&mut self, ui: &mut egui::Ui) {
        let base = data_dir().unwrap_or_default();
        let prefix = base.join("prefix");
        let cache = cache_dir().unwrap_or_default();
        let tree = base.join("wineserver/bin/wineserver").is_file();
        card(ui, |ui| {
            ui.strong("Runtime");
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("wineserver").small());
                ui.label(
                    egui::RichText::new(if tree { "view tree" } else { "Proton's own (no view tree)" })
                        .small()
                        .color(if tree { egui::Color32::from_rgb(87, 199, 133) } else { egui::Color32::from_rgb(232, 179, 74) }),
                );
                ui.add_space(12.0);
                ui.label(egui::RichText::new("prefix").small());
                ui.label(egui::RichText::new(prefix.display().to_string()).small().color(egui::Color32::GRAY));
            });
            ui.horizontal(|ui| {
                if ui.small_button("Open prefix").clicked() {
                    open_path(&prefix);
                }
                if ui.small_button("Open cache").clicked() {
                    open_path(&cache);
                }
                if ui.small_button("Open log").clicked() {
                    if let Some(path) = &self.log_path {
                        open_path(path);
                    }
                }
            });
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
            ui.strong("How should it run?");
            ui.add(
                egui::Label::new(
                    egui::RichText::new("Pick one. The numbers behind it are in Graphics if you want them.")
                        .small()
                        .color(egui::Color32::GRAY),
                ),
            );
            ui.add_space(2.0);
            let mut chosen = None;
            for (preset, title, desc) in [
                (Preset::Recommended, "Recommended", "Measured on your GPU: the steady choice with the right picture."),
                (Preset::LowMemory, "Low memory", "For an 8 GB card or a busy desktop. Textures stream more often."),
                (Preset::Safe, "Safe", "If it crashes or loses the device. A little slower, the most cautious."),
            ] {
                let selected = current == Some(preset);
                let response = ui.add(
                    egui::Button::new(
                        egui::RichText::new(format!("{title}\n{desc}"))
                            .color(if selected { egui::Color32::WHITE } else { egui::Color32::from_rgb(200, 203, 212) }),
                    )
                    .fill(if selected { ACCENT } else { egui::Color32::from_rgb(38, 41, 48) })
                    .min_size(egui::vec2(ui.available_width(), 46.0)),
                );
                if response.clicked() {
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
            ui.add_space(2.0);
            if ui
                .link(egui::RichText::new("Fine tuning…").small())
                .clicked()
            {
                self.tab = Tab::Graphics;
            }
        });
    }

    fn last_run_card(&mut self, ui: &mut egui::Ui) {
        let run = self.last_run();
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Last run");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Bench…").clicked() {
                        self.bench_now();
                    }
                });
            });
            if run.fps.is_empty() && run.removals == 0 {
                ui.label(egui::RichText::new("No run yet.").color(egui::Color32::GRAY));
                return;
            }
            ui.horizontal(|ui| {
                ui.heading(if run.fps.is_empty() { "—" } else { &run.fps });
                ui.label("fps");
                ui.add_space(12.0);
                ui.label(format!("{} ms/frame", run.frame));
                if !run.heap.is_empty() {
                    ui.add_space(12.0);
                    ui.label(format!("heap {} ms", run.heap));
                }
            });
            if !run.phases.is_empty() {
                ui.add(
                    egui::Label::new(egui::RichText::new(&run.phases).small().color(egui::Color32::GRAY)),
                );
            }
            ui.horizontal(|ui| {
                if run.removals == 0 {
                    ui.label(egui::RichText::new("no device removals").small().color(egui::Color32::from_rgb(87, 199, 133)));
                } else {
                    ui.label(egui::RichText::new(format!("{} device removals", run.removals)).small().color(egui::Color32::from_rgb(232, 106, 106)));
                }
                if ui.button("Open log").clicked() {
                    if let Some(path) = &self.log_path {
                        let _ = Command::new(if cfg!(windows) { "notepad" } else { "xdg-open" })
                            .arg(path)
                            .spawn();
                    }
                }
            });
        });
    }

    fn graphics(&mut self, ui: &mut egui::Ui) {
        ui.heading("Graphics");
        ui.add(
            egui::Label::new(
                egui::RichText::new("Fine tuning. Hover a ? for what it does and a safe value.")
                    .small()
                    .color(egui::Color32::GRAY),
            ),
        );
        let warn = warnings(&self.settings, &self.hardware);
        warnings_card(ui, &warn);
        let mut info = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            let sets = &mut self.settings.sets;
            card(ui, |ui| {
                ui.strong("Memory and copies");
                let asked = number_info(ui, sets, "VICTORY_TEXTURE_BUDGET_MB", 0..=8192, 256);
                if info.is_none() { info = asked; }
                let asked = number_info(ui, sets, "VICTORY_CARD_BUFFERS_MB", 0..=4096, 64);
                if info.is_none() { info = asked; }
                let asked = switch_info(ui, sets, "VICTORY_TEXTURE_NO_WAIT");
                if info.is_none() { info = asked; }
                let asked = switch_info(ui, sets, "VICTORY_WIDE_BACK");
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
                ui.strong("Timing (tested, with a history)");
                let asked = number_info(ui, sets, "VICTORY_LATE_WAIT", 0..=3, 1);
                if info.is_none() { info = asked; }
                let asked = switch_info(ui, sets, "VICTORY_JOIN_COPIES");
                if info.is_none() { info = asked; }
                let asked = number_info(ui, sets, "VICTORY_RELAY", 0..=2, 2);
                if info.is_none() { info = asked; }
            });
        });
        if let Some(key) = info {
            self.info = Some(key);
        }
    }

    fn advanced(&mut self, ui: &mut egui::Ui) {
        ui.heading("Advanced");
        egui::ScrollArea::vertical().show(ui, |ui| {
            card(ui, |ui| {
                ui.strong("Every switch");
                ui.add(
                    egui::Label::new(
                        egui::RichText::new("Edit the exact set a run gets. Removing a line turns the switch off.")
                            .small()
                            .color(egui::Color32::GRAY),
                    ),
                );
                let mut remove = None;
                let mut info = None;
                let entries: Vec<(String, String)> = self
                    .settings
                    .sets
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                for (key, value) in entries {
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [300.0, 22.0],
                            egui::Label::new(egui::RichText::new(&key).monospace()),
                        );
                        if describe(&key).is_some() {
                            if ui.small_button("?").on_hover_text("What this does").clicked() {
                                info = Some(key.clone());
                            }
                        } else {
                            ui.add_enabled(false, egui::Button::new("?").small());
                        }
                        if ui.small_button("×").clicked() {
                            remove = Some(key.clone());
                        }
                        let mut v = value.clone();
                        if ui
                            .add_sized([100.0, 22.0], egui::TextEdit::singleline(&mut v))
                            .changed()
                        {
                            self.settings.sets.insert(key.clone(), v);
                        }
                    });
                }
                if let Some(key) = remove {
                    self.settings.sets.remove(&key);
                }
                if let Some(key) = info {
                    self.info = Some(key);
                }
                ui.horizontal(|ui| {
                    ui.label("Add");
                    ui.add(egui::TextEdit::singleline(&mut self.new_key).desired_width(220.0).hint_text("VICTORY_…"));
                    ui.add(egui::TextEdit::singleline(&mut self.new_value).desired_width(80.0).hint_text("1"));
                    if ui.button("Add").clicked() && !self.new_key.trim().is_empty() {
                        self.settings
                            .sets
                            .insert(self.new_key.trim().into(), self.new_value.trim().into());
                        self.new_key.clear();
                        self.new_value.clear();
                    }
                });
            });

            ui.add_space(4.0);
            card(ui, |ui| {
                ui.strong("Extra environment (KEY=VALUE, one per line)");
                ui.add(
                    egui::TextEdit::multiline(&mut self.settings.extra_env)
                        .desired_rows(4)
                        .hint_text("VICTORY_EXAMPLE=1"),
                );
            });

            ui.add_space(4.0);
            card(ui, |ui| {
                ui.strong("Not in this build");
                ui.add(
                    egui::Label::new(
                        egui::RichText::new("Disabled with the reason. A toggle that does nothing is worse than none.")
                            .small()
                            .color(egui::Color32::GRAY),
                    ),
                );
                ui.add_enabled(false, egui::Checkbox::new(&mut self.settings.planned.fsr, "FSR upscaling"));
                ui.add(egui::Label::new(egui::RichText::new("needs AMD's PC FidelityFX DLL and the HLE module").small()));
                ui.add_enabled(false, egui::Checkbox::new(&mut self.settings.planned.ray_tracing, "Ray-traced light"));
                ui.add(egui::Label::new(egui::RichText::new("BVH translates; end-to-end not wired; ~3–4× slower").small()));
                ui.add_enabled(false, egui::Slider::new(&mut self.settings.planned.render_scale, 0.5..=1.0).text("Render scale"));
                ui.add_enabled(false, egui::Checkbox::new(&mut self.settings.planned.vsync, "V-Sync"));
                ui.add_enabled(false, egui::Slider::new(&mut self.settings.planned.fps_cap, 0..=120).text("FPS cap"));
                ui.add_enabled(false, egui::Slider::new(&mut self.settings.planned.lod, 0.5..=2.0).text("LOD distance"));
            });

            ui.add_space(4.0);
            card(ui, |ui| {
                ui.strong("Profile notes");
                if let Some(profile) = self.profile() {
                    ui.label(format!("{} · {}", profile.name, profile.confidence));
                    ui.add(
                        egui::Label::new(egui::RichText::new(&profile.note).small().color(egui::Color32::GRAY)),
                    );
                }
            });
        });
    }

    fn debug(&mut self, ui: &mut egui::Ui) {
        ui.heading("Debug");
        egui::ScrollArea::vertical().show(ui, |ui| {
            card(ui, |ui| {
                ui.strong("Detect");
                ui.horizontal(|ui| {
                    if ui.button("Run --detect").clicked() {
                        self.detect_now();
                    }
                    if self.settings.root.is_some() {
                        ui.label(
                            egui::RichText::new(format!("{} · {} MB", self.hardware.vendor, self.hardware.vram_mb))
                                .small()
                                .color(egui::Color32::GRAY),
                        );
                    }
                });
                if !self.detect_output.is_empty() {
                    egui::ScrollArea::vertical().max_height(130.0).show(ui, |ui| {
                        ui.monospace(&self.detect_output);
                    });
                }
            });

            ui.add_space(4.0);
            card(ui, |ui| {
                ui.strong("Logging and captures");
                ui.horizontal(|ui| {
                    ui.label("Log level");
                    for level in ["error", "warn", "info", "debug", "trace"] {
                        ui.selectable_value(&mut self.settings.log_level, level.to_string(), level);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Capture folder");
                    ui.text_edit_singleline(&mut self.settings.capture_dir);
                });
                ui.horizontal(|ui| {
                    ui.label("Capture frames");
                    ui.text_edit_singleline(&mut self.settings.capture_frames);
                });
                ui.horizontal(|ui| {
                    ui.label("Keys");
                    ui.text_edit_singleline(&mut self.settings.keys);
                });
                ui.add(
                    egui::Label::new(
                        egui::RichText::new("VICTORY_KEYS: seconds:key, e.g. 15:Enter,22:Enter")
                            .small()
                            .color(egui::Color32::GRAY),
                    ),
                );
            });

            ui.add_space(4.0);
            card(ui, |ui| {
                ui.strong("Benchmark and profiling");
                ui.horizontal(|ui| {
                    if ui.button("Benchmark 20 s").clicked() {
                        self.bench_now();
                    }
                    if ui.button("Stop").clicked() {
                        self.stop();
                    }
                    ui.label(
                        egui::RichText::new("runs tools/bench_scene.sh with this profile; median, p10, frame, removals")
                            .small()
                            .color(egui::Color32::GRAY),
                    );
                });
            });
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.refresh_log();
        if self.running() || self.bench_running() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                ui.label(egui::RichText::new(TITLE).size(18.0).strong());
                ui.label(egui::RichText::new(VERSION).small().color(egui::Color32::GRAY));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.running() || self.bench_running() {
                        if ui.add(egui::Button::new("Stop").min_size(egui::vec2(90.0, 32.0))).clicked() {
                            self.stop();
                        }
                    } else if self.tab != Tab::Play && ui.add(primary("Launch", 96.0, 32.0)).clicked() {
                        self.launch();
                    }
                });
            });
        });

        egui::SidePanel::left("rail")
            .exact_width(150.0)
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_space(8.0);
                for (tab, label) in [
                    (Tab::Play, "Play"),
                    (Tab::Graphics, "Graphics"),
                    (Tab::Advanced, "Advanced"),
                    (Tab::Debug, "Debug"),
                ] {
                    if ui
                        .add_sized(
                            [ui.available_width(), 30.0],
                            egui::SelectableLabel::new(self.tab == tab, label),
                        )
                        .clicked()
                    {
                        self.tab = tab;
                    }
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(&self.status).small().color(egui::Color32::GRAY));
                });
            });

        egui::TopBottomPanel::bottom("log")
            .resizable(true)
            .default_height(150.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("Log");
                    if ui.small_button("Clear").clicked() {
                        self.log_shown.clear();
                    }
                    if let Some(path) = &self.log_path {
                        ui.label(egui::RichText::new(path.display().to_string()).small().color(egui::Color32::GRAY));
                    }
                });
                egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                    ui.monospace(&self.log_shown);
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::Play => self.play(ui),
            Tab::Graphics => self.graphics(ui),
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
                            if ui.selectable_label(false, format!("📁 {name}")).clicked() {
                                self.pick_dir = dir;
                            }
                        }
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("Use this folder").clicked() {
                            self.settings.game_dir = self.pick_dir.display().to_string();
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

fn describe(key: &str) -> Option<Desc> {
    let d = |name, plain, safe| Some(Desc { name, plain, safe });
    match key {
        "VICTORY_TEXTURE_BUDGET_MB" => d(
            "Texture memory budget",
            "The most texture data the game may keep on your graphics card at once. Higher can look sharper when you move fast; lower leaves room for other apps.",
            "2048–4096 MB on a 16 GB card. Never more than your card's VRAM.",
        ),
        "VICTORY_CARD_BUFFERS_MB" => d(
            "World memory kept on the card",
            "How much of the game's own working memory is mirrored inside the card. Too much competes with textures.",
            "512 MB measured best on 16 GB; 256 MB on an 8 GB card.",
        ),
        "VICTORY_CARD_BUFFERS_MIN_MB" => d(
            "Smallest buffer kept on the card",
            "Buffers at least this large are mirrored in card memory.",
            "1 MB. Leave it.",
        ),
        "VICTORY_LATE_WAIT" => d(
            "Frames prepared ahead",
            "How many frames the game may hand to the card before the previous one has finished. More can smooth short hitches, but Linux drivers can lose the device.",
            "0 or 1. 2 and above is known to crash on Linux.",
        ),
        "VICTORY_TEXTURE_NO_WAIT" => d(
            "Do not wait for textures",
            "Show the frame without waiting for a texture that is still uploading. Faster; a texture can pop in a moment late.",
            "On. Turn it off only if textures look wrong.",
        ),
        "VICTORY_WIDE_BACK" => d(
            "Copy results back in whole ranges",
            "When shaders write memory the game reads on the processor, copy whole ranges back instead of many small pieces.",
            "On.",
        ),
        "VICTORY_CARD_REHASH_RUNS" => d(
            "Re-check only changed ranges",
            "Only re-read card copies that changed, instead of all of them every frame.",
            "On is safe; the effect is small.",
        ),
        "VICTORY_INDIRECT_WINDOW" => d(
            "Batch indirect draws",
            "Handle many draw records in windows instead of one pass per draw. Measured about 25% faster on this machine.",
            "On.",
        ),
        "VICTORY_INDIRECT_RUNS" => d(
            "Batch single draw records",
            "Clamp a run of single draw records in one pass instead of one pass each. Measured about 18% faster here.",
            "On.",
        ),
        "VICTORY_JOIN_COPIES" => d(
            "Join neighbouring copies",
            "Send adjacent memory copies as one. It has crashed the graphics driver before, so it is off by default.",
            "Off unless you are testing.",
        ),
        "VICTORY_RELAY" => d(
            "Hand results back beside writes",
            "Copy what shaders wrote back into the game's memory next to the write guards. 2 is faster but has crashed before; 0 turns it off.",
            "0, or 2 if your machine stays stable with it.",
        ),
        "VICTORY_LOG" => d(
            "Log detail",
            "How much the layer writes to the log. debug and trace are for bug reports and are much slower.",
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
        if late >= 2 {
            out.push("Frames prepared ahead is 2 or more: this is known to remove the device on Linux. Use 0 or 1.".into());
        }
    }
    if sets.contains_key("VICTORY_JOIN_COPIES") {
        out.push("Joined copies can crash the graphics driver; it did before on this project.".into());
    }
    if setting_value(sets, "VICTORY_RELAY") == Some(2) {
        out.push("Relay 2 has crashed the driver before. If a run dies, set it to 0.".into());
    }
    for key in sets.keys() {
        if key.starts_with("VICTORY_") && describe(key).is_none() {
            out.push(format!("{key} is not a switch this build knows; it will be ignored."));
        }
    }
    out
}

// ---------------------------------------------------------------- widgets

fn primary(text: &str, width: f32, height: f32) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text).strong().color(egui::Color32::WHITE))
        .fill(ACCENT)
        .min_size(egui::vec2(width, height))
}

fn card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(12, 10))
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
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(120, 95, 45)))
        .inner_margin(egui::Margin::symmetric(12, 10))
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
    let mut value = setting_value(sets, key).unwrap_or(*range.start());
    let mut asked = None;
    ui.horizontal(|ui| {
        ui.label(desc.as_ref().map(|d| d.name).unwrap_or(key));
        if desc.is_some() && ui.small_button("?").clicked() {
            asked = Some(key.to_string());
        }
        if ui
            .add(egui::DragValue::new(&mut value).range(range.clone()).speed(step as f64))
            .changed()
        {
            sets.insert(key.to_string(), value.to_string());
        }
        if let Some(d) = &desc {
            ui.label(egui::RichText::new(d.safe.split('.').next().unwrap_or("")).small().color(egui::Color32::GRAY));
        }
    });
    if let Some(d) = &desc {
        ui.add(
            egui::Label::new(egui::RichText::new(d.plain).small().color(egui::Color32::GRAY))
                .wrap(),
        );
    }
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
    ui.horizontal(|ui| {
        if ui.checkbox(&mut on, desc.as_ref().map(|d| d.name).unwrap_or(key)).changed() {
            if on {
                sets.insert(key.to_string(), "1".into());
            } else {
                sets.remove(key);
            }
        }
        if desc.is_some() && ui.small_button("?").clicked() {
            asked = Some(key.to_string());
        }
    });
    if let Some(d) = &desc {
        ui.add(
            egui::Label::new(egui::RichText::new(d.plain).small().color(egui::Color32::GRAY))
                .wrap(),
        );
    }
    asked
}

fn number(
    ui: &mut egui::Ui,
    sets: &mut BTreeMap<String, String>,
    key: &str,
    label: &str,
    range: std::ops::RangeInclusive<u32>,
    step: u32,
    suffix: &str,
) {
    let mut value: u32 = sets
        .get(key)
        .and_then(|v| v.parse().ok())
        .unwrap_or(*range.start());
    ui.horizontal(|ui| {
        ui.label(label);
        if ui
            .add(egui::DragValue::new(&mut value).range(range.clone()).speed(step as f64))
            .changed()
        {
            sets.insert(key.to_string(), value.to_string());
        }
        if !suffix.is_empty() {
            ui.label(egui::RichText::new(suffix).small().color(egui::Color32::GRAY));
        }
    });
}

fn switch(ui: &mut egui::Ui, sets: &mut BTreeMap<String, String>, key: &str, label: &str, note: &str) {
    let mut on = sets.contains_key(key);
    if ui
        .checkbox(&mut on, label)
        .on_hover_text(note)
        .changed()
    {
        if on {
            sets.insert(key.to_string(), "1".into());
        } else {
            sets.remove(key);
        }
    }
}

fn open_path(path: &Path) {
    let _ = Command::new(if cfg!(windows) { "explorer" } else { "xdg-open" })
        .arg(path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
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
