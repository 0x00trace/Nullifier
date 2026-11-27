//! The Controls page: controllers, the keyboard stand-in pad and the mouse.

use eframe::egui;

use super::App;
use crate::keys::{
    key_name, key_name_ok, keys_to_text, load_keys, same_key, scan_controllers, PAD_ACTIONS,
};
use crate::widgets::{card, chip_button, page_header, primary, secondary, small_label};

impl App {
    pub(super) fn controls(&mut self, ui: &mut egui::Ui) {
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
}
