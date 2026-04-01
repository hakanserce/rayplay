//! Settings screen implementation.

use crate::{
    app::{AppScreen, UiApp},
    events::Resolution,
};

/// Shows the settings screen.
pub fn show(ctx: &egui::Context, app: &mut UiApp) {
    egui::CentralPanel::default().show(ctx, |ui| {
        // Header with back button
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            if ui.button("← Back").clicked() {
                app.navigate_to(AppScreen::HostList);
            }
            ui.add_space(20.0);
            ui.heading("Settings");
        });

        ui.add_space(20.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(10.0);

            // Video Settings Section
            show_video_settings_section(ui, app);

            ui.add_space(20.0);

            // Paired Hosts Section
            show_paired_hosts_section(ui, app);

            ui.add_space(20.0);

            // About Section
            show_about_section(ui);

            ui.add_space(40.0);
        });
    });
}

/// Shows the video settings section.
fn show_video_settings_section(ui: &mut egui::Ui, app: &mut UiApp) {
    egui::Frame::none()
        .fill(egui::Color32::WHITE)
        .stroke(egui::Stroke::new(2.0, egui::Color32::from_gray(221)))
        .rounding(8.0)
        .inner_margin(20.0)
        .show(ui, |ui| {
            ui.label(egui::RichText::new("Video").size(18.0).strong());

            ui.add_space(15.0);

            // Resolution setting
            ui.horizontal(|ui| {
                ui.label("Resolution");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    egui::ComboBox::from_id_salt("resolution")
                        .selected_text(app.video_settings.resolution.display_name())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut app.video_settings.resolution,
                                Resolution::Hd1080,
                                Resolution::Hd1080.display_name(),
                            );
                            ui.selectable_value(
                                &mut app.video_settings.resolution,
                                Resolution::Qhd1440,
                                Resolution::Qhd1440.display_name(),
                            );
                            ui.selectable_value(
                                &mut app.video_settings.resolution,
                                Resolution::Uhd4K,
                                Resolution::Uhd4K.display_name(),
                            );
                            ui.selectable_value(
                                &mut app.video_settings.resolution,
                                Resolution::MatchHost,
                                Resolution::MatchHost.display_name(),
                            );
                        });
                });
            });

            ui.add_space(15.0);

            // Frame rate setting
            ui.horizontal(|ui| {
                ui.label("Frame Rate");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    egui::ComboBox::from_id_salt("fps")
                        .selected_text(format!("{} fps", app.video_settings.fps))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut app.video_settings.fps, 30, "30 fps");
                            ui.selectable_value(&mut app.video_settings.fps, 60, "60 fps");
                            ui.selectable_value(&mut app.video_settings.fps, 120, "120 fps");
                            ui.selectable_value(&mut app.video_settings.fps, 144, "144 fps");
                        });
                });
            });

            ui.add_space(15.0);

            // Bitrate setting
            ui.horizontal(|ui| {
                ui.label("Bitrate");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("{} Mbps", app.video_settings.bitrate));
                    ui.add_space(10.0);
                    ui.add_sized(
                        [150.0, 20.0],
                        egui::Slider::new(&mut app.video_settings.bitrate, 5..=100),
                    );
                });
            });
        });
}

/// Shows the paired hosts section.
fn show_paired_hosts_section(ui: &mut egui::Ui, app: &mut UiApp) {
    egui::Frame::none()
        .fill(egui::Color32::WHITE)
        .stroke(egui::Stroke::new(2.0, egui::Color32::from_gray(221)))
        .rounding(8.0)
        .inner_margin(20.0)
        .show(ui, |ui| {
            ui.label(egui::RichText::new("Paired Hosts").size(18.0).strong());

            ui.add_space(15.0);

            // Collect paired host indices first to avoid borrow checker issues
            let paired_host_indices: Vec<usize> = app
                .hosts
                .iter()
                .enumerate()
                .filter_map(|(i, host)| if host.paired { Some(i) } else { None })
                .collect();

            if paired_host_indices.is_empty() {
                ui.label(
                    egui::RichText::new("No paired hosts").color(egui::Color32::from_gray(102)),
                );
            } else {
                let mut revoke_index: Option<usize> = None;

                for (display_index, &host_index) in paired_host_indices.iter().enumerate() {
                    if display_index > 0 {
                        ui.separator();
                        ui.add_space(10.0);
                    }

                    if let Some(host) = app.hosts.get(host_index) {
                        ui.horizontal(|ui| {
                            ui.label(format!("{} ({}:{})", host.name, host.address, host.port));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .button(
                                            egui::RichText::new("Revoke")
                                                .color(egui::Color32::WHITE),
                                        )
                                        .clicked()
                                    {
                                        revoke_index = Some(host_index);
                                    }
                                },
                            );
                        });

                        if display_index < paired_host_indices.len() - 1 {
                            ui.add_space(10.0);
                        }
                    }
                }

                // Process revocation outside of the iterator
                if let Some(index) = revoke_index
                    && let Some(host) = app.hosts.get_mut(index)
                {
                    host.paired = false;
                }
            }
        });
}

/// Shows the about section.
fn show_about_section(ui: &mut egui::Ui) {
    egui::Frame::none()
        .fill(egui::Color32::WHITE)
        .stroke(egui::Stroke::new(2.0, egui::Color32::from_gray(221)))
        .rounding(8.0)
        .inner_margin(20.0)
        .show(ui, |ui| {
            ui.label(egui::RichText::new("About").size(18.0).strong());

            ui.add_space(15.0);

            ui.horizontal(|ui| {
                ui.label("RayView Version");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label("1.0.0-beta.1");
                });
            });
        });
}
