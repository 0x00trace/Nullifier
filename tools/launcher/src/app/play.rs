//! The Play page: the game folder, the profile, the launch button and the last run.

use std::path::{Path, PathBuf};
use std::time::Duration;

use eframe::egui;

use super::{current_preset, App, Pick, Prep, Preset, Tab};
use crate::hardware::vram_label;
use crate::install::{pipeline_cache, title_exe};
use crate::os::{home_dir, open_path};
use crate::profiles::auto_profile;
use crate::run_log::tail;
use crate::settings::{cache_dir, data_dir};
use crate::switches::warnings;
use crate::widgets::{
    card, chip_button, page_header, pair, preset_card, primary, secondary, stat, warnings_card,
};

impl App {
    pub(super) fn play(&mut self, ui: &mut egui::Ui) {
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

    pub(super) fn runtime_card(&mut self, ui: &mut egui::Ui) {
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
                (
                    Preset::Safe,
                    "Safe",
                    "Waits for textures and turns send-ahead off. Slower; no black pop-in.",
                ),
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
                        sets.insert("VICTORY_TEXTURE_NO_WAIT".into(), "0".into());
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
}
