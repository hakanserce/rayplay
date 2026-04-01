//! Streaming screen implementation with overlay menu.

use crate::{
    app::{AppScreen, UiApp},
    events::UiAction,
};

/// Shows the streaming screen with video area and overlay controls.
pub fn show(ctx: &egui::Context, app: &mut UiApp) {
    // Full-screen black background for video
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(egui::Color32::from_gray(26)))
        .show(ctx, |ui| {
            // Video area placeholder
            let video_rect = ui.available_rect_before_wrap();
            ui.painter()
                .rect_filled(video_rect, 0.0, egui::Color32::from_gray(42));

            ui.painter().text(
                video_rect.center(),
                egui::Align2::CENTER_CENTER,
                "[Game Stream Video Area]",
                egui::FontId::proportional(24.0),
                egui::Color32::from_gray(102),
            );

            // Reconnecting banner at the top
            if app.reconnecting {
                show_reconnecting_banner(ui, app);
            }

            // Floating action button (hamburger menu) at top-right
            show_fab_button(ui, app);

            // Stream menu dropdown
            if app.streaming_menu_open {
                show_stream_menu(ui, app);
            }
        });
}

/// Shows the reconnecting banner at the top of the screen.
fn show_reconnecting_banner(ui: &mut egui::Ui, app: &mut UiApp) {
    let banner_rect = egui::Rect::from_min_size(
        ui.available_rect_before_wrap().min,
        egui::Vec2::new(ui.available_width(), 50.0),
    );

    // Semi-transparent background
    ui.painter().rect_filled(
        banner_rect,
        0.0,
        egui::Color32::from_rgba_premultiplied(0, 0, 0, 192),
    );

    let mut banner_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(banner_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );

    banner_ui.add_space(20.0);

    // Pulsing yellow dot
    banner_ui.painter().circle_filled(
        banner_ui.cursor().center() + egui::Vec2::new(10.0, 0.0),
        5.0,
        egui::Color32::from_rgb(255, 165, 0), // Orange
    );

    banner_ui.add_space(30.0);

    banner_ui.label(
        egui::RichText::new(format!(
            "Reconnecting... ({}s remaining)",
            app.reconnect_countdown
        ))
        .color(egui::Color32::WHITE),
    );

    banner_ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.add_space(20.0);
        if ui
            .button(egui::RichText::new("Cancel").color(egui::Color32::WHITE))
            .clicked()
        {
            app.send_action(UiAction::Disconnect);
            app.navigate_to(AppScreen::HostList);
        }
    });
}

/// Shows the floating action button for the menu.
fn show_fab_button(ui: &mut egui::Ui, app: &mut UiApp) {
    let fab_pos = ui.available_rect_before_wrap().max - egui::Vec2::new(64.0, 64.0);
    let fab_rect = egui::Rect::from_center_size(fab_pos, egui::Vec2::splat(44.0));

    let response = ui.allocate_rect(fab_rect, egui::Sense::click());

    if response.clicked() {
        app.streaming_menu_open = !app.streaming_menu_open;
    }

    // Semi-transparent background with blur effect (simulated)
    let fab_color = if response.hovered() {
        egui::Color32::from_rgba_premultiplied(255, 255, 255, 76)
    } else {
        egui::Color32::from_rgba_premultiplied(255, 255, 255, 38)
    };

    ui.painter().circle(
        fab_rect.center(),
        22.0,
        fab_color,
        egui::Stroke::new(
            1.0,
            egui::Color32::from_rgba_premultiplied(255, 255, 255, 64),
        ),
    );

    // Hamburger icon
    ui.painter().text(
        fab_rect.center(),
        egui::Align2::CENTER_CENTER,
        "☰",
        egui::FontId::proportional(20.0),
        egui::Color32::WHITE,
    );
}

/// Shows the stream menu dropdown.
fn show_stream_menu(ui: &mut egui::Ui, app: &mut UiApp) {
    let menu_pos = ui.available_rect_before_wrap().max - egui::Vec2::new(280.0, 120.0);
    let menu_rect = egui::Rect::from_min_size(menu_pos, egui::Vec2::new(260.0, 200.0));

    // Semi-transparent background
    ui.painter().rect_filled(
        menu_rect,
        egui::CornerRadius::same(10),
        egui::Color32::from_rgba_premultiplied(30, 30, 30, 235),
    );

    ui.painter().rect_stroke(
        menu_rect,
        egui::CornerRadius::same(10),
        egui::Stroke::new(
            1.0,
            egui::Color32::from_rgba_premultiplied(255, 255, 255, 38),
        ),
        egui::StrokeKind::Middle,
    );

    let mut menu_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(menu_rect.shrink(16.0))
            .layout(egui::Layout::top_down(egui::Align::LEFT)),
    );

    // Header with host info
    menu_ui.label(
        egui::RichText::new(&app.target_host_name)
            .size(15.0)
            .strong()
            .color(egui::Color32::WHITE),
    );

    if let Some(stats) = &app.stream_stats {
        menu_ui.label(
            egui::RichText::new(format!(
                "{} @ {}fps · {} · {:.1}ms",
                stats.resolution, stats.fps, stats.codec, stats.latency_ms
            ))
            .size(12.0)
            .family(egui::FontFamily::Monospace)
            .color(egui::Color32::from_gray(170)),
        );
    }

    // Connection status
    menu_ui.horizontal(|ui| {
        let status_color = if app.reconnecting {
            egui::Color32::from_rgb(255, 165, 0) // Orange
        } else {
            egui::Color32::from_rgb(80, 200, 120) // Green
        };

        ui.painter().circle_filled(
            ui.cursor().center() + egui::Vec2::new(5.0, 8.0),
            5.0,
            status_color,
        );

        ui.add_space(16.0);

        let status_text = if app.reconnecting {
            "Reconnecting..."
        } else {
            "Connected"
        };

        ui.label(
            egui::RichText::new(status_text)
                .size(13.0)
                .color(egui::Color32::from_gray(170)),
        );
    });

    menu_ui.add_space(10.0);
    menu_ui.separator();
    menu_ui.add_space(10.0);

    // Menu items
    if menu_ui
        .button(
            egui::RichText::new(if app.fullscreen {
                "Exit Fullscreen"
            } else {
                "Go Fullscreen"
            })
            .color(egui::Color32::from_gray(221)),
        )
        .clicked()
    {
        app.send_action(UiAction::ToggleFullscreen);
        app.fullscreen = !app.fullscreen;
        app.streaming_menu_open = false;
    }

    if menu_ui
        .button(egui::RichText::new("Settings").color(egui::Color32::from_gray(221)))
        .clicked()
    {
        app.navigate_to(AppScreen::Settings);
        app.streaming_menu_open = false;
    }

    menu_ui.add_space(5.0);
    menu_ui.separator();
    menu_ui.add_space(5.0);

    if menu_ui
        .button(egui::RichText::new("Disconnect").color(egui::Color32::from_rgb(231, 76, 60)))
        .clicked()
    {
        app.send_action(UiAction::Disconnect);
        app.navigate_to(AppScreen::HostList);
        app.streaming_menu_open = false;
    }
}
