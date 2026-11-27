//! The launcher's window: its state, the work it starts (a run, a benchmark, a
//! build) and the frame that draws the rail and the open page. Each page is a
//! child module.

mod advanced;
mod community;
mod controls;
mod debug;
mod graphics;
mod play;
mod rendering;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use eframe::egui;

use crate::board::{load_community, normalize_community, CommunityEntry};
use crate::game_settings::{load_game_settings, GameSettings};
use crate::hardware::{detect, hardware, Hardware};
use crate::install::{engine_exe, settings_root_readiness, title_exe, Readiness};
use crate::keys::load_keys;
use crate::launch::{custom_profile, kill_game, launch_command};
use crate::os::{home_dir, open_path, open_url};
use crate::profiles::{profiles, Profile};
use crate::run_log::{
    after, before, first_num, last_num, median, median_fps, megabytes, percentile, read_tail,
    strip_ansi, tail, world_fps, world_frames, world_loaded, LastRun,
};
use crate::settings::{cache_dir, profile_hash, Settings};
use crate::switches::describe;
use crate::update::{is_newer, update_check, RELEASES_URL};
use crate::widgets::{capitalise, chip_button, nav_group, nav_item, primary};
use crate::{ACCENT, LAYER_VERSION, VERSION};

pub(crate) struct App {
    pub(crate) settings: Settings,
    profiles: Vec<Profile>,
    hardware: Hardware,
    pub(crate) tab: Tab,
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
    pub(crate) info: Option<String>,
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

fn current_preset(
    sets: &BTreeMap<String, String>,
    shipped: &BTreeMap<String, String>,
) -> Option<Preset> {
    let safe = sets.get("VICTORY_LATE_WAIT").map(String::as_str) == Some("0")
        && sets.get("VICTORY_TEXTURE_NO_WAIT").map(String::as_str) == Some("0")
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
pub(crate) enum Tab {
    Play,
    Graphics,
    Controls,
    Community,
    Rendering,
    Advanced,
    Debug,
}

impl App {
    pub(crate) fn new(settings: Settings) -> Self {
        let profiles = settings.root.as_deref().map(profiles).unwrap_or_default();
        let hardware = settings.root.as_deref().map(hardware).unwrap_or_default();
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

    pub(crate) fn bench_now(&mut self) {
        if self.running() {
            self.status = "close the game before measuring".into();
            return;
        }
        let Some(root) = self.settings.root.clone() else {
            return;
        };
        let Some(profile) = custom_profile(&self.settings) else {
            return;
        };
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
        let Some(child) = self.prep.as_mut() else {
            return;
        };
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
                    (Prep::Wineserver, false) => {
                        "the fast graphics service did not build - the log has the details".into()
                    }
                    (Prep::Preload, true) => "shaders preloaded".into(),
                    (Prep::Preload, false) => {
                        "preloading stopped early - the log has the details".into()
                    }
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
        let Some(start) = self.bench_start else {
            return;
        };
        let elapsed = start.elapsed().as_secs_f32();
        let text = self
            .log_path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        let seen = world_frames(&text);
        if self.bench_world.is_none() {
            let loaded = seen.len() >= 2
                && world_loaded(&seen[seen.len() - 1])
                && world_loaded(&seen[seen.len() - 2]);
            if loaded {
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
                let mut values = world_fps(slice);
                if values.is_empty() {
                    values = seen.iter().map(|f| f.fps).collect();
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
        let Some(path) = &self.log_path else {
            return run;
        };
        let Some(text) = read_tail(path, 2 * 1024 * 1024) else {
            return run;
        };
        run.removals = text.matches("device removed").count() as u32;
        let mut world: Vec<f32> = Vec::new();
        let mut menus: Vec<f32> = Vec::new();
        let mut last_fps = None;
        for line in text.lines() {
            let clean = strip_ansi(line);
            let Some(rest) = after(&clean, "host: ") else {
                continue;
            };
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
        self.profiles
            .iter()
            .find(|p| p.name == self.settings.profile)
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
        self.poll_work(ctx);

        self.take_dropped_folder(ctx);

        self.rail(ctx);

        self.log_footer(ctx);

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(23, 25, 31))
                    .inner_margin(egui::Margin::symmetric(22, 18)),
            )
            .show(ctx, |ui| match self.tab {
                Tab::Play => self.play(ui),
                Tab::Graphics => self.game_page(ui),
                Tab::Controls => self.controls(ui),
                Tab::Community => self.community_page(ui),
                Tab::Rendering => self.rendering(ui),
                Tab::Advanced => self.advanced(ui),
                Tab::Debug => self.debug(ui),
            });

        self.info_window(ctx);

        self.folder_picker(ctx);
    }
}

/// The parts of the window that `update` draws, in the order it draws them.
impl App {
    /// Takes the results of the board fetch and the update check, moves the
    /// benchmark and the build steps on, and asks for a repaint while work runs.
    fn poll_work(&mut self, ctx: &egui::Context) {
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
    }

    /// A folder (or the .exe) dropped anywhere on the window becomes the game
    /// folder.
    fn take_dropped_folder(&mut self, ctx: &egui::Context) {
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
                Some(exe) => format!(
                    "found {}",
                    exe.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("the game")
                ),
                None => "that folder has no .exe in it".into(),
            };
            self.toast = Some((self.status.clone(), std::time::Instant::now()));
        }
    }

    /// The rail on the left: the name, the versions and the page buttons.
    fn rail(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("rail")
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(27, 30, 38))
                    .inner_margin(egui::Margin::symmetric(14, 14))
                    .stroke(egui::Stroke::new(
                        1.0_f32,
                        egui::Color32::from_rgb(42, 46, 56),
                    )),
            )
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
                                        egui::RichText::new("● update").size(10.0).color(ACCENT),
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
    }

    /// The status line at the bottom, and the log when it is open.
    fn log_footer(&mut self, ctx: &egui::Context) {
        let footer = egui::TopBottomPanel::bottom("log")
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(27, 30, 38))
                    .inner_margin(egui::Margin::symmetric(16, 8))
                    .stroke(egui::Stroke::new(
                        1.0_f32,
                        egui::Color32::from_rgb(42, 46, 56),
                    )),
            )
            .resizable(false);
        let footer = if self.log_open {
            footer.exact_height(180.0)
        } else {
            footer
        };
        footer.show(ctx, |ui| {
            ui.horizontal(|ui| {
                let live = self.running() || self.bench_running();
                ui.label(
                    egui::RichText::new(capitalise(&self.status))
                        .small()
                        .color(if live {
                            egui::Color32::from_rgb(87, 199, 133)
                        } else {
                            egui::Color32::GRAY
                        }),
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
                    if chip_button(
                        ui,
                        if self.log_open {
                            "Hide log"
                        } else {
                            "Show log"
                        },
                    )
                    .clicked()
                    {
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
                                    ui.selectable_value(
                                        &mut self.settings.log_level,
                                        value.to_string(),
                                        label,
                                    );
                                }
                            });
                        ui.label(
                            egui::RichText::new("Log detail")
                                .small()
                                .color(egui::Color32::GRAY),
                        );
                    }
                });
            });
            if self.log_open {
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        ui.monospace(&self.log_shown);
                    });
            }
            ui.add_space(4.0);
        });
    }

    /// The window that explains a switch, open while `info` names one.
    fn info_window(&mut self, ctx: &egui::Context) {
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
    }

    /// The window that picks the game folder, open while `picking` is set.
    fn folder_picker(&mut self, ctx: &egui::Context) {
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
                    egui::ScrollArea::vertical()
                        .max_height(260.0)
                        .show(ui, |ui| {
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
                                    egui::RichText::new(label)
                                        .color(egui::Color32::from_rgb(160, 214, 180))
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
                        if ui.add(primary("Use this folder", 140.0, 32.0)).clicked() {
                            let picked = self.pick_dir.display().to_string();
                            match self.pick_for {
                                Pick::Game => {
                                    self.settings.game_dir = picked;
                                    self.settings.save();
                                    self.tab = Tab::Play;
                                    self.status = match title_exe(&self.pick_dir) {
                                        Some(exe) => format!(
                                            "found {}",
                                            exe.file_name()
                                                .and_then(|n| n.to_str())
                                                .unwrap_or("the game")
                                        ),
                                        None => {
                                            "game_scarlett_master.exe is not in that folder".into()
                                        }
                                    };
                                    if title_exe(&self.pick_dir).is_some() {
                                        self.toast =
                                            Some((self.status.clone(), std::time::Instant::now()));
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
