//! The Community page: the board of shared runs, and sharing the last run.

use std::collections::BTreeMap;

use eframe::egui;

use super::{App, Tab};
use crate::board::{
    date_of, fetch_board, game_label, sanitize_log, save_community, world_stats, CommunityEntry,
};
use crate::hardware::vram_label;
use crate::switches::settings_summary;
use crate::widgets::{
    card, chip_button, page_header, primary, secondary, small_label, table_cell, table_sep,
};
use crate::{LAYER_VERSION, VERSION};

/// A shared run's name and its `VICTORY_*` settings, as "Use" hands them to
/// `use_settings`.
type NamedSets = (String, BTreeMap<String, String>);

impl App {
    pub(super) fn save_community_after_fetch(&self) {
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

    pub(super) fn keep_settings(&mut self) {
        self.borrowed_sets = None;
        self.borrowed_profile = None;
        self.borrowed_from = None;
        self.settings.save();
        self.status = "settings kept".into();
    }

    pub(super) fn revert_settings(&mut self) {
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

    pub(super) fn community_page(&mut self, ui: &mut egui::Ui) {
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
            card(ui, |ui| self.entry_card(ui));

            ui.add_space(4.0);
            card(ui, |ui| self.board_card(ui));
        });

        self.entry_window(ui);
    }

    /// The "Your entry" card: the name, place and notes shared with a run, and
    /// the button that shares the last run.
    fn entry_card(&mut self, ui: &mut egui::Ui) {
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
                        egui::TextEdit::singleline(&mut self.settings.community_name)
                            .hint_text("a nickname")
                            .vertical_align(egui::Align::Center),
                    )
                    .changed();
                ui.end_row();
                ui.label(small_label("Where"));
                changed |= ui
                    .add_sized(
                        [380.0, 28.0],
                        egui::TextEdit::singleline(&mut self.settings.community_area)
                            .hint_text("prologue cell, boat mission…")
                            .vertical_align(egui::Align::Center),
                    )
                    .changed();
                ui.end_row();
                ui.label(small_label("Notes"));
                changed |= ui
                    .add_sized(
                        [380.0, 28.0],
                        egui::TextEdit::singleline(&mut self.settings.community_notes)
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
                        if in_world {
                            "in the world"
                        } else {
                            "(last log, maybe a menu)"
                        },
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
    }

    /// The "Shared runs" card: the counts, the filters and the table.
    fn board_card(&mut self, ui: &mut egui::Ui) {
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
        let mut games: Vec<String> = self
            .community
            .iter()
            .map(|entry| entry.game.clone())
            .collect();
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
        let mut versions: Vec<String> = self
            .community
            .iter()
            .map(|entry| entry.version.clone())
            .collect();
        versions.sort();
        versions.dedup();
        let active_filters = usize::from(!self.community_game.is_empty())
            + usize::from(!self.community_area_filter.is_empty())
            + usize::from(!self.community_version_filter.is_empty())
            + usize::from(self.community_notes_only);
        self.board_filters(ui, games, areas, versions, active_filters);
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
        let (open, borrow) = self.board_table(ui, shown);
        if let Some((name, sets)) = borrow {
            self.use_settings(&name, &sets);
        }
        self.community_open = open;
    }

    /// The table of shared runs that pass the filters, in the chosen order.
    /// Returns the run to open and the settings to borrow, when a row asks for
    /// either.
    fn board_table(
        &mut self,
        ui: &mut egui::Ui,
        shown: Vec<usize>,
    ) -> (Option<usize>, Option<NamedSets>) {
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
        (open, borrow)
    }

    /// The search field, the filter menu and the sort menu above the table.
    /// `games`, `areas` and `versions` are the choices the filter menu lists.
    fn board_filters(
        &mut self,
        ui: &mut egui::Ui,
        games: Vec<String>,
        areas: Vec<String>,
        versions: Vec<String>,
        active_filters: usize,
    ) {
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
                            .selectable_label(self.community_version_filter.is_empty(), "All")
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
            let sort_text = if self.community_sort == 1 {
                "Fastest"
            } else {
                "Newest"
            };
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
    }

    /// The window that shows one shared run, open while `community_open` names
    /// it: its details, and buttons to use its settings or remove it.
    fn entry_window(&mut self, ui: &mut egui::Ui) {
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
}
