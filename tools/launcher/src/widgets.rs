//! Drawing helpers that several pages use: table cells, buttons, the rail, page
//! headers, cards and the rows that describe a switch.

use std::collections::BTreeMap;

use eframe::egui;

use crate::switches::{describe, example, setting_value};
use crate::ACCENT;

/// One fixed-width table cell, left-aligned, the same call in the header and rows.
pub(crate) fn table_cell(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, 20.0),
        egui::Layout::left_to_right(egui::Align::Center),
        add,
    );
}

pub(crate) fn table_sep(ui: &mut egui::Ui) {
    ui.add(egui::Separator::default().vertical().spacing(8.0));
}

pub(crate) fn small_label(text: &str) -> egui::RichText {
    egui::RichText::new(text)
        .small()
        .color(egui::Color32::from_rgb(140, 146, 158))
}

pub(crate) fn danger_remove(ui: &mut egui::Ui) -> egui::Response {
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

pub(crate) fn preset_card(ui: &mut egui::Ui, selected: bool, title: &str, desc: &str) -> egui::Response {
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

pub(crate) fn pair(ms: Option<f32>, mb: Option<f32>) -> Option<String> {
    match (ms, mb) {
        (Some(ms), Some(mb)) => Some(format!("{ms:.1} ms · {mb:.0} MB")),
        (Some(ms), None) => Some(format!("{ms:.1} ms")),
        _ => None,
    }
}

pub(crate) fn stat(ui: &mut egui::Ui, label: &str, value: Option<String>) {
    ui.vertical(|ui| {
        ui.set_min_width(150.0);
        ui.label(egui::RichText::new(label).small().color(egui::Color32::GRAY));
        ui.label(value.unwrap_or_else(|| "—".into()));
    });
}

pub(crate) fn chip_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(text).small())
            .corner_radius(4)
            .min_size(egui::vec2(0.0, 26.0)),
    )
}

pub(crate) fn help_button(ui: &mut egui::Ui) -> egui::Response {
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

pub(crate) fn secondary(text: &str, width: f32, height: f32) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text).color(egui::Color32::from_rgb(220, 224, 233)))
        .fill(egui::Color32::from_rgb(43, 48, 59))
        .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(57, 64, 78)))
        .corner_radius(7)
        .min_size(egui::vec2(width, height))
}

pub(crate) fn primary(text: &str, width: f32, height: f32) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text).strong().color(egui::Color32::WHITE))
        .fill(ACCENT)
        .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(105, 145, 245)))
        .corner_radius(7)
        .min_size(egui::vec2(width, height))
}

pub(crate) fn nav_group(ui: &mut egui::Ui, label: &str) {
    ui.add_space(10.0);
    ui.label(
        egui::RichText::new(label)
            .size(9.5)
            .strong()
            .color(egui::Color32::from_rgb(112, 118, 130)),
    );
    ui.add_space(2.0);
}

pub(crate) fn nav_item(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
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

pub(crate) fn page_header(ui: &mut egui::Ui, title: &str, subtitle: &str) {
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

pub(crate) fn card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
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

pub(crate) fn warnings_card(ui: &mut egui::Ui, list: &[String]) {
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
pub(crate) fn number_info(
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
pub(crate) fn switch_info(
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

pub(crate) fn section_header(ui: &mut egui::Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(egui::RichText::new(title).strong().size(15.0));
    ui.separator();
}

pub(crate) fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
