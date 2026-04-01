//! Add/edit host screen implementation.

use crate::{
    app::{AppScreen, UiApp},
    host::HostEntry,
};
use std::net::IpAddr;

/// Shows the add/edit host form.
pub fn show(ctx: &egui::Context, app: &mut UiApp) {
    egui::CentralPanel::default().show(ctx, |ui| {
        // Header with back button
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            if ui.button("← Back").clicked() {
                app.navigate_to(AppScreen::HostList);
            }
        });

        ui.add_space(30.0);

        // Center the form
        ui.vertical_centered(|ui| {
            egui::Frame::none()
                .fill(egui::Color32::WHITE)
                .stroke(egui::Stroke::new(2.0, egui::Color32::from_gray(221)))
                .rounding(8.0)
                .inner_margin(30.0)
                .show(ui, |ui| {
                    ui.set_max_width(400.0);

                    let title = if app.editing_host().is_some() {
                        "Edit Host"
                    } else {
                        "Add New Host"
                    };

                    ui.vertical_centered(|ui| {
                        ui.heading(title);
                    });

                    ui.add_space(30.0);

                    // Get current values (either from editing host or defaults)
                    let (mut name, mut address_str, mut port_str) =
                        if let Some(host) = app.editing_host() {
                            (
                                host.name.clone(),
                                host.address.to_string(),
                                host.port.to_string(),
                            )
                        } else {
                            (String::new(), String::new(), "7860".to_string())
                        };

                    // Name field
                    ui.label("Name");
                    ui.add_space(5.0);
                    ui.text_edit_singleline(&mut name);
                    ui.add_space(20.0);

                    // Address field
                    ui.label("IP Address / Hostname");
                    ui.add_space(5.0);
                    ui.text_edit_singleline(&mut address_str);
                    ui.add_space(20.0);

                    // Port field
                    ui.label("Port");
                    ui.add_space(5.0);
                    ui.text_edit_singleline(&mut port_str);
                    ui.add_space(30.0);

                    // Buttons
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // Save button
                            let can_save = !name.is_empty()
                                && !address_str.is_empty()
                                && port_str.parse::<u16>().is_ok()
                                && address_str.parse::<IpAddr>().is_ok();

                            ui.add_enabled_ui(can_save, |ui| {
                                if ui.button("Save").clicked()
                                    && let (Ok(address), Ok(port)) =
                                        (address_str.parse::<IpAddr>(), port_str.parse::<u16>())
                                {
                                    let host = HostEntry::new(name, address, port);
                                    app.add_host(host);
                                }
                            });

                            ui.add_space(10.0);

                            // Cancel button
                            if ui.button("Cancel").clicked() {
                                app.navigate_to(AppScreen::HostList);
                            }
                        });
                    });
                });
        });
    });
}
