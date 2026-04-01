//! Connecting screen implementation.

use crate::app::{AppScreen, UiApp};

/// Shows the connecting screen with spinner.
pub fn show(ctx: &egui::Context, app: &mut UiApp) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.vertical_centered_justified(|ui| {
            ui.add_space(ui.available_height() * 0.3);

            egui::Frame::none()
                .fill(egui::Color32::WHITE)
                .stroke(egui::Stroke::new(2.0, egui::Color32::from_gray(221)))
                .rounding(8.0)
                .inner_margin(50.0)
                .show(ui, |ui| {
                    ui.set_max_width(400.0);

                    ui.vertical_centered(|ui| {
                        // Spinner
                        ui.add_space(20.0);
                        ui.spinner();
                        ui.add_space(20.0);

                        // Connecting message
                        ui.heading(format!("Connecting to {}...", app.target_host_name));

                        ui.add_space(20.0);

                        ui.label(
                            egui::RichText::new("Please wait while we establish the connection")
                                .color(egui::Color32::from_gray(102)),
                        );

                        ui.add_space(40.0);

                        // Cancel button
                        if ui.button("Cancel").clicked() {
                            app.send_action(crate::events::UiAction::Disconnect);
                            app.navigate_to(AppScreen::HostList);
                        }
                    });
                });
        });
    });
}
