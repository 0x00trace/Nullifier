//! The Tools page (`Tab::Debug`): hardware, the benchmark, the installation and
//! screenshots.

use std::path::PathBuf;

use eframe::egui;

use super::{App, Pick};
use crate::hardware::vram_label;
use crate::os::home_dir;
use crate::profiles::auto_profile;
use crate::widgets::{card, page_header};

impl App {
    pub(super) fn debug(&mut self, ui: &mut egui::Ui) {
        page_header(ui, "Tools", "Hardware, benchmark, installation and screenshots.");
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.spacing_mut().interact_size.y = 28.0;
            card(ui, |ui| {
                ui.strong("This computer");
                ui.horizontal(|ui| {
                    ui.label(if self.hardware.vendor.is_empty() {
                        "Not checked yet".to_string()
                    } else {
                        format!(
                            "{} · {} of graphics memory",
                            self.hardware.vendor.to_uppercase(),
                            vram_label(self.hardware.vram_mb)
                        )
                    });
                    if ui
                        .button("Scan hardware")
                        .on_hover_text("Checks your graphics card and picks the right profile.")
                        .clicked()
                    {
                        self.detect_now();
                        let auto = auto_profile(&self.profiles, &self.hardware);
                        self.load_profile(&auto);
                    }
                });
                egui::CollapsingHeader::new("Details")
                    .default_open(false)
                    .show(ui, |ui| {
                        if self.detect_output.is_empty() {
                            ui.label(egui::RichText::new("Scan to see the details.").small());
                        } else {
                            ui.monospace(&self.detect_output);
                        }
                    });
            });

            ui.add_space(4.0);
            card(ui, |ui| {
                ui.strong("Benchmark");
                ui.add(
                    egui::Label::new(
                        egui::RichText::new("Runs the game for 20 seconds with these settings and reports the frame rate. Takes about two minutes.")
                            .small()
                            .color(egui::Color32::GRAY),
                    )
                    .wrap(),
                );
                ui.horizontal(|ui| {
                    let busy = self.bench_running();
                    let playing = self.running();
                    if ui
                        .add_enabled(!busy && !playing, egui::Button::new("Run benchmark"))
                        .clicked()
                    {
                        self.bench_now();
                    }
                    if busy {
                        ui.label(egui::RichText::new("Running…").small());
                    } else if playing {
                        ui.label(egui::RichText::new("Close the game first.").small().color(egui::Color32::GRAY));
                    }
                });
                if self.bench_progress > 0.0 {
                    ui.add(
                        egui::ProgressBar::new(self.bench_progress)
                            .text(&self.bench_phase)
                            .animate(self.bench_running()),
                    );
                }
                if !self.bench_result.is_empty() {
                    ui.label(
                        egui::RichText::new(&self.bench_result)
                            .small()
                            .color(egui::Color32::from_rgb(87, 199, 133)),
                    );
                }
            });

            ui.add_space(4.0);
            self.runtime_card(ui);

            ui.add_space(4.0);
            ui.add_space(4.0);
            card(ui, |ui| {
                ui.strong("Screenshots");
                let mut save = !self.settings.capture_dir.trim().is_empty();
                if ui.checkbox(&mut save, "Take screenshots").changed() {
                    if save {
                        if self.settings.capture_dir.trim().is_empty() {
                            self.settings.capture_dir = home_dir()
                                .map(|h| h.join("Pictures/nullifier").display().to_string())
                                .unwrap_or_default();
                        }
                    } else {
                        self.settings.capture_dir.clear();
                    }
                }
                if save {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new("A still picture at each frame number below — one PNG each, not a video.")
                                .small()
                                .color(egui::Color32::GRAY),
                        )
                        .wrap(),
                    );
                    ui.horizontal(|ui| {
                        ui.label("Folder");
                        ui.text_edit_singleline(&mut self.settings.capture_dir);
                        if ui.button("Choose…").clicked() {
                            self.pick_for = Pick::Capture;
                            self.pick_dir = PathBuf::from(&self.settings.capture_dir);
                            if !self.pick_dir.is_dir() {
                                self.pick_dir = home_dir().unwrap_or_else(|| PathBuf::from("/"));
                            }
                            self.picking = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Frame numbers");
                        ui.add_sized(
                            [140.0, 22.0],
                            egui::TextEdit::singleline(&mut self.settings.capture_frames),
                        )
                        .on_hover_text("Counted from the first frame the game draws. 1024 is about a minute in.");
                    });
                }
            });

            ui.add_space(4.0);
        });
    }
}
