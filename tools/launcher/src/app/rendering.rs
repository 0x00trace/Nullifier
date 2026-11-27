//! The Rendering page: the layer's own settings for speed, memory and stability.

use eframe::egui;

use super::App;
use crate::switches::warnings;
use crate::widgets::{card, number_info, page_header, switch_info, warnings_card};

impl App {
    pub(super) fn rendering(&mut self, ui: &mut egui::Ui) {
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
        let mut frame_changed = false;
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
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Title frame rate");
                    let current = match sets
                        .get("VICTORY_FRAME_INTERVAL_MS")
                        .map(String::as_str)
                    {
                        Some("16.667") => "60 fps",
                        Some("8.333") => "120 fps",
                        Some("6.944") => "144 fps",
                        Some("0") => "Unlimited",
                        _ => "Title's 30 fps",
                    };
                    egui::ComboBox::from_id_salt("title frame rate")
                        .selected_text(current)
                        .width(160.0)
                        .show_ui(ui, |ui| {
                            for (label, value) in [
                                ("Title's 30 fps", None),
                                ("60 fps", Some("16.667")),
                                ("120 fps", Some("8.333")),
                                ("144 fps", Some("6.944")),
                                ("Unlimited", Some("0")),
                            ] {
                                let selected = match value {
                                    None => !sets.contains_key("VICTORY_FRAME_INTERVAL_MS"),
                                    Some(v) => {
                                        sets.get("VICTORY_FRAME_INTERVAL_MS")
                                            .is_some_and(|now| now == v)
                                    }
                                };
                                if ui.selectable_label(selected, label).clicked() && !selected {
                                    match value {
                                        Some(v) => {
                                            sets.insert(
                                                "VICTORY_FRAME_INTERVAL_MS".into(),
                                                v.into(),
                                            );
                                        }
                                        None => {
                                            sets.remove("VICTORY_FRAME_INTERVAL_MS");
                                        }
                                    }
                                    frame_changed = true;
                                }
                            }
                        });
                });
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(
                            "Patches the title's own 30 fps setting and makes our frame clock follow. Off leaves the title at 30. The world must be able to reach the rate to see a difference.",
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
                let asked = switch_info(ui, sets, "VICTORY_WATER_LIGHT");
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
        if frame_changed {
            self.settings.save();
        }
        if let Some(key) = info {
            self.info = Some(key);
        }
    }
}
