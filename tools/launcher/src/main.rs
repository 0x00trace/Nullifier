//! Nullifier's graphical launcher.
//!
//! Three tabs: Play (hardware, profile, game folder), Graphics (the switches
//! that exist and are measured) and Debug (logging, captures and a free-form
//! environment). The title's own Xbox menu never had any of these.
//!
//! The switches it edits are handed to the shell launcher as a temporary
//! profile (`VICTORY_PROFILES`), so a switch that is turned *off* is off by
//! omission — no variable is ever set to a value its reader might take as
//! "on". Entries under "not in this build" are deliberately inert: FSR, ray
//! tracing, resolution scale, vsync, an FPS cap and LOD distance need engine
//! work (see docs/ROADMAP.md), and a toggle that lies is worse than none.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use eframe::egui;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const TITLE: &str = "Nullifier launcher";

fn main() -> eframe::Result<()> {
    let root = find_root();
    let mut settings = Settings::load(root.as_deref());

    let start_tab = std::env::args()
        .position(|a| a == "--tab")
        .and_then(|i| std::env::args().nth(i + 1));
    if std::env::args().any(|a| a == "--check") {
        check(root.as_deref(), &settings);
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([980.0, 700.0])
            .with_min_inner_size([760.0, 520.0])
            .with_title(TITLE),
        ..Default::default()
    };
    eframe::run_native(
        TITLE,
        options,
        Box::new(move |_cc| {
            settings.root = root.clone();
            let mut app = App::new(settings);
            app.tab = match start_tab.as_deref() {
                Some("graphics") => Tab::Graphics,
                Some("debug") => Tab::Debug,
                _ => Tab::Play,
            };
            Ok(Box::new(app))
        }),
    )
}

// ---------------------------------------------------------------- settings

#[derive(Clone, Serialize, Deserialize)]
struct Settings {
    game_dir: String,
    /// "" is auto: the profile the launcher detects.
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

/// Toggles that need engine work; kept in the file so the choice survives,
/// but never applied.
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
            let profiles = profiles(root);
            if settings.sets.is_empty() {
                let name = default_profile(&profiles);
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

// ---------------------------------------------------------------- profiles

struct Profile {
    name: String,
    sets: BTreeMap<String, String>,
    note: String,
    os: String,
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
                            let v = v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string());
                            sets.insert(k.clone(), v);
                        }
                    }
                    Some(Profile {
                        name,
                        sets,
                        note: p.get("note").and_then(|n| n.as_str()).unwrap_or("").to_string(),
                        os: p
                            .get("match")
                            .and_then(|m| m.get("os"))
                            .and_then(|o| o.as_str())
                            .unwrap_or("")
                            .to_string(),
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

fn default_profile(profiles: &[Profile]) -> String {
    let os = if cfg!(windows) { "windows" } else { "linux" };
    profiles
        .iter()
        .find(|p| p.os == os)
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

fn detect(root: &Path) -> String {
    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("powershell");
        c.args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
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

fn custom_profile(settings: &Settings) -> Option<PathBuf> {
    let dir = cache_dir()?;
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join("launcher-profile.json");
    let os = if cfg!(windows) { "windows" } else { "linux" };
    let doc = json!({
        "version": 1,
        "_comment": ["written by the Nullifier launcher on every run"],
        "defaults": {},
        "profiles": [{
            "name": "custom",
            "match": { "os": os, "vendor": "any" },
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

/// The title sees Windows paths; a Linux path becomes `Z:\...`.
fn capture_path(dir: &str) -> String {
    if cfg!(windows) {
        dir.to_string()
    } else {
        format!("Z:{}", dir.replace('/', "\\"))
    }
}

fn check(root: Option<&Path>, settings: &Settings) {
    println!("{TITLE}");
    match root {
        Some(root) => println!("repo: {}", root.display()),
        None => println!("repo: not found (set NULLIFIER_ROOT)"),
    }
    println!("game: {}", settings.game_dir);
    println!("profile: {}", if settings.profile.is_empty() { "auto" } else { &settings.profile });
    println!("sets:");
    for (k, v) in &settings.sets {
        println!("  {k}={v}");
    }
    if let Some(root) = root {
        if let Some(profile) = custom_profile(settings) {
            println!("profile file: {}", profile.display());
            let cmd = launch_command(root, settings, &profile);
            println!("command: {:?} {:?}", cmd.get_program(), cmd.get_args().collect::<Vec<_>>());
        }
    }
    println!("planned (not applied): {}", serde_json::to_string(&settings.planned).unwrap());
}

// ---------------------------------------------------------------- the app

struct App {
    settings: Settings,
    profiles: Vec<Profile>,
    tab: Tab,
    detect_output: String,
    child: Option<Child>,
    log_path: Option<PathBuf>,
    log_shown: String,
    status: String,
    confirm_unsafe: bool,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Tab {
    Play,
    Graphics,
    Debug,
}

impl App {
    fn new(settings: Settings) -> Self {
        let profiles = settings.root.as_deref().map(profiles).unwrap_or_default();
        Self {
            settings,
            profiles,
            tab: Tab::Play,
            detect_output: String::new(),
            child: None,
            log_path: None,
            log_shown: String::new(),
            status: "ready".into(),
            confirm_unsafe: false,
        }
    }

    fn running(&mut self) -> bool {
        match self.child.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => false,
        }
    }

    fn launch(&mut self) {
        let Some(root) = self.settings.root.clone() else {
            self.status = "repo root not found (set NULLIFIER_ROOT)".into();
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
                self.status = "launched".into();
                self.tab = Tab::Play;
            }
            Err(e) => self.status = format!("cannot launch: {e}"),
        }
    }

    fn stop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
        }
        self.child = None;
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
            let tail: String = text
                .lines()
                .rev()
                .take(120)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n");
            self.log_shown = tail;
        }
    }

    fn load_profile(&mut self, name: &str) {
        if let Some(profile) = self.profiles.iter().find(|p| p.name == name) {
            self.settings.profile = name.to_string();
            self.settings.sets = profile.sets.clone();
            self.status = format!("loaded profile {}", name);
        }
    }

    /// The switches with a measured reason behind them, in one place.
    fn graphics(&mut self, ui: &mut egui::Ui) {
        ui.heading("Graphics");
        ui.label(
            "These are the switches this build actually reads. Off means the \
             variable is absent, so the engine's own default applies.",
        );
        ui.add_space(6.0);

        let sets = &mut self.settings.sets;
        ui.group(|ui| {
            ui.strong("Memory and copies");
            number(ui, sets, "VICTORY_TEXTURE_BUDGET_MB", "Texture budget (MB)", 0..=8192, 256,
                   "Caps texture copies on the card. 4096 is the shipped value; the texture phase fell 350 -> 40 ms.");
            number(ui, sets, "VICTORY_CARD_BUFFERS_MB", "Card buffers (MB)", 0..=4096, 64,
                   "Title memory kept in the card's memory. 512 is the shipped value.");
            switch(ui, sets, "VICTORY_TEXTURE_NO_WAIT", "Do not wait for textures",
                   "Under Wine this has been the faster choice.");
            switch(ui, sets, "VICTORY_WIDE_BACK", "Wide copy-back", "Copy back whole ranges instead of pieces.");
            switch(ui, sets, "VICTORY_CARD_REHASH_RUNS", "Rehash changed runs", "Only re-hash card runs that changed.");
        });

        ui.add_space(4.0);
        ui.group(|ui| {
            ui.strong("Draw path");
            switch(ui, sets, "VICTORY_INDIRECT_WINDOW", "Indirect draw windows",
                   "Clamp records in windows instead of one pass per draw. Measured +25% here.");
            switch(ui, sets, "VICTORY_INDIRECT_RUNS", "Indirect runs",
                   "One clamp pass per run of single records. Measured +18% here.");
        });

        ui.add_space(4.0);
        ui.group(|ui| {
            ui.strong("Timing (measured, but with a history)");
            number(ui, sets, "VICTORY_LATE_WAIT", "Send-ahead (LATE_WAIT)", 0..=3, 1,
                   "0 or 1. 2 and above remove the device on Linux.");
            switch(ui, sets, "VICTORY_JOIN_COPIES", "Join copies",
                   "Has removed the device before the view-tree wineserver; test it.");
            number(ui, sets, "VICTORY_RELAY", "Relay (0 or 2)", 0..=2, 2,
                   "Copy-back relay; has removed the device on Linux.");
        });

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("Shipped").clicked() {
                let name = default_profile(&self.profiles);
                self.load_profile(&name);
            }
            if ui.button("Safe").clicked() {
                let sets = &mut self.settings.sets;
                sets.insert("VICTORY_LATE_WAIT".into(), "0".into());
                sets.remove("VICTORY_JOIN_COPIES");
                sets.remove("VICTORY_RELAY");
                self.status = "safe: LATE_WAIT=0, no joined copies, no relay".into();
            }
            if ui.button("Fast").clicked() {
                let sets = &mut self.settings.sets;
                for key in [
                    "VICTORY_TEXTURE_NO_WAIT",
                    "VICTORY_WIDE_BACK",
                    "VICTORY_INDIRECT_WINDOW",
                    "VICTORY_INDIRECT_RUNS",
                    "VICTORY_CARD_REHASH_RUNS",
                ] {
                    sets.insert(key.into(), "1".into());
                }
                sets.insert("VICTORY_LATE_WAIT".into(), "1".into());
                self.status = "fast: the shipped measured wins, all on".into();
            }
        });
    }

    fn debug(&mut self, ui: &mut egui::Ui) {
        ui.heading("Debug");
        ui.group(|ui| {
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
            ui.label("Keys (VICTORY_KEYS): seconds:key, e.g. 15:Enter,22:Enter");
            ui.text_edit_singleline(&mut self.settings.keys);
        });

        ui.add_space(4.0);
        ui.group(|ui| {
            ui.strong("Extra environment (KEY=VALUE, one per line)");
            ui.add(
                egui::TextEdit::multiline(&mut self.settings.extra_env)
                    .desired_rows(4)
                    .hint_text("VICTORY_EXAMPLE=1"),
            );
        });

        ui.add_space(4.0);
        ui.group(|ui| {
            ui.strong("Not in this build");
            ui.label("Kept here so the intent is written down. They are inert: a toggle that does nothing is worse than none.");
            ui.add_enabled(false, egui::Checkbox::new(&mut self.settings.planned.fsr, "FSR upscaling"));
            ui.label("   needs AMD's PC FidelityFX DLL and the HLE module; not ported yet.");
            ui.add_enabled(false, egui::Checkbox::new(&mut self.settings.planned.ray_tracing, "Ray-traced light"));
            ui.label("   the title's BVH programs translate, but the end-to-end path is not wired; ~3-4x slower.");
            ui.add_enabled(false, egui::Slider::new(&mut self.settings.planned.render_scale, 0.5..=1.0).text("Render scale"));
            ui.label("   needs a swapchain override in the layer.");
            ui.add_enabled(false, egui::Checkbox::new(&mut self.settings.planned.vsync, "V-Sync"));
            ui.add_enabled(false, egui::Slider::new(&mut self.settings.planned.fps_cap, 0..=120).text("FPS cap"));
            ui.label("   Present() takes no interval or pacing yet.");
            ui.add_enabled(false, egui::Slider::new(&mut self.settings.planned.lod, 0.5..=2.0).text("LOD distance"));
            ui.label("   the title's LOD selection has no hook of ours.");
        });

        ui.add_space(4.0);
        ui.group(|ui| {
            ui.strong("Detect");
            if ui.button("Run --detect").clicked() {
                if let Some(root) = &self.settings.root {
                    self.detect_output = detect(root);
                }
            }
            if !self.detect_output.is_empty() {
                egui::ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
                    ui.monospace(&self.detect_output);
                });
            }
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.refresh_log();
        if self.running() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(TITLE);
                ui.label(format!("v{}", env!("CARGO_PKG_VERSION")));
                ui.separator();
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.running() {
                        if ui.button("Stop").clicked() {
                            self.stop();
                        }
                    } else if ui.button("Launch").clicked() {
                        self.launch();
                    }
                });
            });
        });

        egui::SidePanel::left("tabs").resizable(false).show(ctx, |ui| {
            ui.add_space(6.0);
            ui.selectable_value(&mut self.tab, Tab::Play, "Play");
            ui.selectable_value(&mut self.tab, Tab::Graphics, "Graphics");
            ui.selectable_value(&mut self.tab, Tab::Debug, "Debug");
        });

        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::Play => {
                ui.heading("Play");
                ui.horizontal(|ui| {
                    ui.label("Game folder");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.settings.game_dir)
                            .desired_width(520.0)
                            .hint_text("the folder holding the title's files"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Profile");
                    let current = if self.settings.profile.is_empty() {
                        "auto".to_string()
                    } else {
                        self.settings.profile.clone()
                    };
                    let names: Vec<String> =
                        self.profiles.iter().map(|p| p.name.clone()).collect();
                    let mut picked: Option<String> = None;
                    egui::ComboBox::from_id_salt("profile")
                        .selected_text(current)
                        .show_ui(ui, |ui| {
                            for name in &names {
                                if ui
                                    .selectable_label(&self.settings.profile == name, name)
                                    .clicked()
                                {
                                    picked = Some(name.clone());
                                }
                            }
                        });
                    if let Some(name) = picked {
                        self.load_profile(&name);
                    }
                    if ui.button("Detect hardware").clicked() {
                        if let Some(root) = &self.settings.root {
                            self.detect_output = detect(root);
                            if let Some(line) = self.detect_output.lines().find(|l| l.contains("profile")) {
                                if let Some(after) = line.split_whitespace().nth(1) {
                                    let name = after.to_string();
                                    if self.profiles.iter().any(|p| p.name == name) {
                                        self.load_profile(&name);
                                    }
                                }
                            }
                        }
                    }
                });
                if let Some(profile) = self.profiles.iter().find(|p| p.name == self.settings.profile) {
                    ui.label(format!("{} profile: {}", profile.confidence, profile.name));
                    if !profile.note.is_empty() {
                        egui::CollapsingHeader::new("what this profile was measured on")
                            .default_open(false)
                            .show(ui, |ui| {
                                ui.add(egui::Label::new(egui::RichText::new(&profile.note).small()));
                            });
                    }
                }
                if !self.detect_output.is_empty() {
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                        ui.monospace(&self.detect_output);
                    });
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(if self.running() { "Running" } else { "Launch" }).clicked()
                        && !self.running()
                    {
                        self.launch();
                    }
                    if ui.button("Save").clicked() {
                        self.settings.save();
                        self.status = "saved".into();
                    }
                });
                ui.label("Launch runs the same launcher script as the command line, with this profile.");
            }
            Tab::Graphics => self.graphics(ui),
            Tab::Debug => self.debug(ui),
        });

        egui::TopBottomPanel::bottom("log").resizable(true).default_height(160.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Log");
                if ui.small_button("Clear").clicked() {
                    self.log_shown.clear();
                }
                if let Some(path) = &self.log_path {
                    ui.small(path.display().to_string());
                }
            });
            egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                ui.monospace(&self.log_shown);
            });
        });
    }
}

// ---------------------------------------------------------------- widgets

fn number(
    ui: &mut egui::Ui,
    sets: &mut BTreeMap<String, String>,
    key: &str,
    label: &str,
    range: std::ops::RangeInclusive<u32>,
    step: u32,
    note: &str,
) {
    let mut value: u32 = sets.get(key).and_then(|v| v.parse().ok()).unwrap_or(*range.start());
    ui.horizontal(|ui| {
        ui.label(label);
        if ui
            .add(egui::DragValue::new(&mut value).range(range.clone()).speed(step as f64))
            .changed()
        {
            sets.insert(key.to_string(), value.to_string());
        }
    });
    if !note.is_empty() {
        ui.add(egui::Label::new(egui::RichText::new(note).small()));
    }
}

fn switch(
    ui: &mut egui::Ui,
    sets: &mut BTreeMap<String, String>,
    key: &str,
    label: &str,
    note: &str,
) {
    let mut on = sets.contains_key(key);
    if ui.checkbox(&mut on, label).changed() {
        if on {
            sets.insert(key.to_string(), "1".into());
        } else {
            sets.remove(key);
        }
    }
    if !note.is_empty() {
        ui.add(egui::Label::new(egui::RichText::new(note).small()));
    }
}
