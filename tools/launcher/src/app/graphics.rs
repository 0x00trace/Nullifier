//! The Graphics page: the title's own graphics settings, read from and written to
//! its `settings.xml`.

use eframe::egui;

use super::App;
use crate::game_settings::{
    apply_game, choices, float_key, float_range, int_slider, is_curated, label_of, limits,
    load_game_settings, no_effect, section_title, GameSetting, GameSettings, DISPLAY_KEYS,
    GRAPHICS_KEYS, SHADOW_KEYS,
};
use crate::switches::example;
use crate::widgets::{card, page_header, primary, secondary, section_header};

impl App {
    pub(super) fn game_page(&mut self, ui: &mut egui::Ui) {
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
                        "Experimental — only gamma is confirmed to change the picture. Values marked no effect changed nothing in tests.",
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
        ui.label(
            egui::RichText::new(&game.note)
                .small()
                .color(egui::Color32::GRAY),
        );
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
            .on_hover_text(
                "In the 2026-11-27 measurements this changed neither frame rate, draws nor copy-back. Only gamma is confirmed to change the picture.",
            );
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
