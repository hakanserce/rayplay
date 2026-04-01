//! Host list screen implementation.

use crate::{
    app::{AppScreen, UiApp},
    host::HostBadge,
};

/// Shows the main host list screen.
pub fn show(ctx: &egui::Context, app: &mut UiApp) {
    egui::CentralPanel::default().show(ctx, |ui| {
        // Header with title and settings button
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            ui.heading("RayView");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(20.0);
                if ui.button("⚙").clicked() {
                    app.navigate_to(AppScreen::Settings);
                }
            });
        });

        ui.add_space(30.0);

        // Host grid
        let available_width = ui.available_width() - 40.0; // Account for margins
        let card_width = 250.0;
        let spacing = 20.0;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let _columns = ((available_width + spacing) / (card_width + spacing))
            .floor()
            .max(1.0) as usize;

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = spacing;
                ui.spacing_mut().item_spacing.y = spacing;

                let mut clicked_index = None;
                let host_count = app.hosts.len();

                for index in 0..host_count {
                    let host = &app.hosts[index];
                    let response = ui.allocate_response(
                        egui::Vec2::new(card_width, 120.0),
                        egui::Sense::click(),
                    );

                    if response.clicked() {
                        clicked_index = Some(index);
                    }

                    // Draw host card
                    let rect = response.rect;
                    let visuals = ui.style().interact(&response);

                    ui.painter().rect(
                        rect,
                        8.0,
                        egui::Color32::WHITE,
                        egui::Stroke::new(2.0, visuals.bg_stroke.color),
                    );

                    // Host content
                    let content_rect = rect.shrink(20.0);
                    let mut content_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(content_rect)
                            .layout(egui::Layout::top_down(egui::Align::LEFT)),
                    );

                    // Host name
                    content_ui.label(egui::RichText::new(&host.name).size(16.0).strong());

                    // IP address
                    content_ui.label(
                        egui::RichText::new(format!("{}:{}", host.address, host.port))
                            .size(12.0)
                            .color(egui::Color32::from_gray(102))
                            .family(egui::FontFamily::Monospace),
                    );

                    // Badges
                    content_ui.horizontal(|ui| {
                        for badge in host.badges() {
                            show_badge(ui, badge);
                        }
                    });

                    // Last connected
                    content_ui.label(
                        egui::RichText::new(host.last_connected_display())
                            .size(11.0)
                            .color(egui::Color32::from_gray(153)),
                    );
                }

                // Handle deferred host click
                if let Some(index) = clicked_index {
                    show_host_popup(app, index);
                }

                // Add host button
                let add_response =
                    ui.allocate_response(egui::Vec2::new(card_width, 120.0), egui::Sense::click());

                if add_response.clicked() {
                    app.navigate_to(AppScreen::AddHost);
                }

                let add_rect = add_response.rect;
                let add_visuals = ui.style().interact(&add_response);

                ui.painter().rect(
                    add_rect,
                    8.0,
                    egui::Color32::WHITE,
                    egui::Stroke::new(2.0, add_visuals.bg_stroke.color),
                );

                // Draw + symbol
                let center = add_rect.center();
                ui.painter().text(
                    center,
                    egui::Align2::CENTER_CENTER,
                    "+",
                    egui::FontId::proportional(48.0),
                    add_visuals.text_color(),
                );
            });
        });
    });
}

/// Shows a badge for a host.
fn show_badge(ui: &mut egui::Ui, badge: HostBadge) {
    let text = badge.text();
    let color = badge.color();

    let text_galley = ui.painter().layout_no_wrap(
        text.to_string(),
        egui::FontId::proportional(10.0),
        egui::Color32::WHITE,
    );
    let text_size = text_galley.size();
    let padding = egui::Vec2::splat(4.0);
    let badge_size = text_size + 2.0 * padding;

    let (rect, _) = ui.allocate_exact_size(badge_size, egui::Sense::hover());

    ui.painter()
        .rect_filled(rect, egui::Rounding::same(6.0), color);

    ui.painter().galley(
        rect.center() - text_size / 2.0,
        text_galley,
        egui::Color32::WHITE,
    );
}

/// Shows the popup menu for a host (simplified implementation for now).
fn show_host_popup(app: &mut UiApp, host_index: usize) {
    // For now, just connect directly when clicked
    // TODO: Implement proper popup menu
    app.connect_to_host(host_index);
}
