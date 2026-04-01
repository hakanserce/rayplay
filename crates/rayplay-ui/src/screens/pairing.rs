//! PIN entry screen for pairing.

use crate::{
    app::{AppScreen, UiApp},
    events::UiAction,
};

/// Shows the PIN entry screen for pairing.
pub fn show(ctx: &egui::Context, app: &mut UiApp) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            if ui.button("← Back").clicked() {
                app.navigate_to(AppScreen::HostList);
            }
        });

        ui.add_space(50.0);

        ui.vertical_centered(|ui| {
            egui::Frame::new()
                .fill(egui::Color32::WHITE)
                .stroke(egui::Stroke::new(2.0, egui::Color32::from_gray(221)))
                .corner_radius(8)
                .inner_margin(30.0)
                .show(ui, |ui| {
                    ui.set_max_width(400.0);
                    ui.vertical_centered(|ui| {
                        ui.heading(format!("Pair with {}", app.target_host_name));
                    });
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new("Enter the 6-digit PIN displayed on the host.")
                            .size(13.0)
                            .color(egui::Color32::from_gray(136)),
                    );
                    ui.add_space(20.0);
                    show_pin_boxes(ui, app);
                    ui.add_space(20.0);
                    show_pairing_status(ui, app);
                    ui.vertical_centered(|ui| {
                        if ui.button("Cancel").clicked() {
                            app.navigate_to(AppScreen::HostList);
                        }
                    });
                });
        });
    });
}

/// Renders the 6-digit PIN input boxes.
fn show_pin_boxes(ui: &mut egui::Ui, app: &mut UiApp) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;

        while app.pin_input.len() < 6 {
            app.pin_input.push(' ');
        }
        app.pin_input.truncate(6);

        let mut chars: Vec<char> = app.pin_input.chars().collect();

        for i in 0..6 {
            let mut single_char = chars.get(i).copied().unwrap_or(' ').to_string();
            if single_char == " " {
                single_char.clear();
            }

            let response = ui.add_sized(
                [50.0, 50.0],
                egui::TextEdit::singleline(&mut single_char)
                    .font(egui::TextStyle::Heading)
                    .horizontal_align(egui::Align::Center),
            );

            if single_char.len() > 1 {
                single_char.truncate(1);
            }

            if let Some(new_char) = single_char.chars().next() {
                if new_char.is_ascii_digit() {
                    chars[i] = new_char;
                    if i < 5 {
                        ui.memory_mut(|mem| mem.request_focus(response.id.with(i + 1)));
                    }
                } else {
                    chars[i] = ' ';
                }
            } else {
                chars[i] = ' ';
            }

            if response.has_focus() {
                let events = ui.input(|input| input.events.clone());
                for event in &events {
                    if let egui::Event::Key {
                        key: egui::Key::Backspace,
                        pressed: true,
                        ..
                    } = event
                    {
                        chars[i] = ' ';
                        if i > 0 {
                            ui.memory_mut(|mem| mem.request_focus(response.id.with(i - 1)));
                        }
                    }
                }
            }
        }

        app.pin_input = chars.iter().collect::<String>().replace(' ', "");
    });

    // Auto-submit when PIN is complete
    if app.pin_input.len() == 6
        && app.pin_input.chars().all(|c| c.is_ascii_digit())
        && app.pairing_status.is_empty()
    {
        app.pairing_status = "Verifying...".to_string();
        app.send_action(UiAction::SubmitPin(app.pin_input.clone()));
    }
}

/// Shows pairing status text.
fn show_pairing_status(ui: &mut egui::Ui, app: &UiApp) {
    if app.pairing_status.is_empty() {
        ui.add_space(40.0);
    } else {
        ui.vertical_centered(|ui| {
            let color = if app.pairing_status.contains("successful") {
                egui::Color32::from_rgb(80, 200, 120)
            } else {
                egui::Color32::from_rgb(231, 76, 60)
            };
            ui.label(egui::RichText::new(&app.pairing_status).color(color));
        });
        ui.add_space(20.0);
    }
}
