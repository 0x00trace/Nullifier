//! The Advanced page: the exact set of switches and the environment, for
//! troubleshooting.

use eframe::egui;

use super::App;
use crate::switches::{describe, example, readable_value, KNOWN_SWITCHES};
use crate::widgets::{card, danger_remove, help_button, page_header, small_label};

impl App {
    pub(super) fn advanced(&mut self, ui: &mut egui::Ui) {
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
}
