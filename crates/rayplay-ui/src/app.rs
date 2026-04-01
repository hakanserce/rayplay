//! Main UI application and state machine.

use crate::{
    events::{UiAction, UiEvent, VideoSettings},
    host::HostEntry,
    screens,
};
use crossbeam_channel::{Receiver, Sender};

/// Main UI application state.
///
/// Manages the current screen, host list, and communication channels.
pub struct UiApp {
    /// Current active screen.
    pub current_screen: AppScreen,
    /// List of known hosts.
    pub hosts: Vec<HostEntry>,
    /// Video streaming settings.
    pub video_settings: VideoSettings,
    /// Channel receiver for events from the network thread.
    pub event_rx: Receiver<UiEvent>,
    /// Channel sender for actions to the network thread.
    pub action_tx: Sender<UiAction>,
    /// Currently editing host index (for `AddHost` screen).
    pub editing_host_index: Option<usize>,
    /// Current PIN input for pairing.
    pub pin_input: String,
    /// Status text for pairing screen.
    pub pairing_status: String,
    /// Host name for connecting/pairing screens.
    pub target_host_name: String,
    /// Whether the streaming menu is open.
    pub streaming_menu_open: bool,
    /// Whether we're currently in fullscreen mode.
    pub fullscreen: bool,
    /// Whether we're currently reconnecting.
    pub reconnecting: bool,
    /// Reconnection countdown in seconds.
    pub reconnect_countdown: u32,
    /// Current stream statistics.
    pub stream_stats: Option<StreamStats>,
}

/// Current screen in the UI state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppScreen {
    /// Main host list screen.
    HostList,
    /// Add/edit host form screen.
    AddHost,
    /// PIN entry for pairing.
    Pairing,
    /// Connection in progress.
    Connecting,
    /// Currently streaming.
    Streaming,
    /// Settings screen.
    Settings,
}

/// Current stream statistics.
#[derive(Debug, Clone)]
pub struct StreamStats {
    /// Resolution string like "1920x1080".
    pub resolution: String,
    /// Frame rate.
    pub fps: u32,
    /// Codec name.
    pub codec: String,
    /// Latency in milliseconds.
    pub latency_ms: u32,
}

impl UiApp {
    /// Creates a new UI application.
    #[must_use]
    pub fn new(event_rx: Receiver<UiEvent>, action_tx: Sender<UiAction>) -> Self {
        Self {
            current_screen: AppScreen::HostList,
            hosts: Self::create_sample_hosts(),
            video_settings: VideoSettings::default(),
            event_rx,
            action_tx,
            editing_host_index: None,
            pin_input: String::new(),
            pairing_status: String::new(),
            target_host_name: String::new(),
            streaming_menu_open: false,
            fullscreen: false,
            reconnecting: false,
            reconnect_countdown: 0,
            stream_stats: None,
        }
    }

    /// Creates some sample hosts for development/demonstration.
    fn create_sample_hosts() -> Vec<HostEntry> {
        use chrono::{Duration, Utc};
        use std::net::IpAddr;

        let mut hosts = Vec::new();

        // Gaming PC - paired, connected recently
        let mut gaming_pc = HostEntry::new(
            "Gaming PC".to_string(),
            IpAddr::V4(std::net::Ipv4Addr::new(192, 168, 1, 100)),
            7860,
        );
        gaming_pc.paired = true;
        gaming_pc.last_connected = Some(Utc::now() - Duration::hours(2));
        hosts.push(gaming_pc);

        // Work Laptop - unpaired, discovered
        let mut work_laptop = HostEntry::new(
            "Work Laptop".to_string(),
            IpAddr::V4(std::net::Ipv4Addr::new(192, 168, 1, 105)),
            7860,
        );
        work_laptop.discovered = true;
        hosts.push(work_laptop);

        // Media Server - paired, older connection
        let mut media_server = HostEntry::new(
            "Media Server".to_string(),
            IpAddr::V4(std::net::Ipv4Addr::new(192, 168, 1, 50)),
            7860,
        );
        media_server.paired = true;
        media_server.last_connected = Some(Utc::now() - Duration::days(3));
        hosts.push(media_server);

        hosts
    }

    /// Processes all pending events from the network thread.
    pub fn process_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            self.handle_event(event);
        }
    }

    /// Handles a single UI event.
    fn handle_event(&mut self, event: UiEvent) {
        match event {
            UiEvent::PinRequired { host_name } => {
                self.target_host_name = host_name;
                self.current_screen = AppScreen::Pairing;
                self.pin_input.clear();
                self.pairing_status.clear();
            }
            UiEvent::Connected { host_name } => {
                self.target_host_name = host_name;
                self.current_screen = AppScreen::Streaming;
                self.reconnecting = false;

                // Mark the host as connected
                if let Some(host) = self
                    .hosts
                    .iter_mut()
                    .find(|h| h.name == self.target_host_name)
                {
                    host.mark_connected();
                }
            }
            UiEvent::Reconnecting {
                host_name,
                countdown_secs,
            } => {
                self.target_host_name = host_name;
                self.reconnecting = true;
                self.reconnect_countdown = countdown_secs;
                // Stay on Streaming screen but show reconnect banner
            }
            UiEvent::Disconnected { error: _ } => {
                // Return to host list regardless of which screen we were on
                self.current_screen = AppScreen::HostList;
                self.reconnecting = false;
                self.streaming_menu_open = false;
                self.fullscreen = false;
            }
            UiEvent::StreamStats {
                resolution,
                fps,
                codec,
                latency_ms,
            } => {
                self.stream_stats = Some(StreamStats {
                    resolution,
                    fps,
                    codec,
                    latency_ms,
                });
            }
            UiEvent::PairingResult { success, error } => {
                if success {
                    self.pairing_status = "Pairing successful!".to_string();
                    // Mark host as paired
                    if let Some(host) = self
                        .hosts
                        .iter_mut()
                        .find(|h| h.name == self.target_host_name)
                    {
                        host.mark_paired();
                    }
                    // Will transition to Connecting/Streaming via Connected event
                } else {
                    self.pairing_status = error.unwrap_or_else(|| "Pairing failed".to_string());
                }
            }
        }
    }

    /// Sends an action to the network thread.
    pub fn send_action(&self, action: UiAction) {
        if let Err(e) = self.action_tx.send(action) {
            tracing::warn!("Failed to send UI action: {}", e);
        }
    }

    /// Main UI update function called each frame.
    pub fn update(&mut self, ctx: &egui::Context) {
        self.process_events();

        match &self.current_screen {
            AppScreen::HostList => screens::host_list::show(ctx, self),
            AppScreen::AddHost => screens::add_host::show(ctx, self),
            AppScreen::Pairing => screens::pairing::show(ctx, self),
            AppScreen::Connecting => screens::connecting::show(ctx, self),
            AppScreen::Streaming => screens::streaming::show(ctx, self),
            AppScreen::Settings => screens::settings::show(ctx, self),
        }
    }

    /// Returns whether the UI wants to consume input events.
    ///
    /// When false, input should be passed through to the game stream.
    #[must_use]
    pub fn wants_input(&self) -> bool {
        match &self.current_screen {
            AppScreen::Streaming => {
                // Only consume input when menu is open or we're reconnecting
                self.streaming_menu_open || self.reconnecting
            }
            _ => true, // All other screens consume input
        }
    }

    /// Navigates to a specific screen.
    pub fn navigate_to(&mut self, screen: AppScreen) {
        // Clear screen-specific state when navigating
        match &screen {
            AppScreen::HostList => {
                self.editing_host_index = None;
            }
            AppScreen::Pairing => {
                self.pin_input.clear();
                self.pairing_status.clear();
            }
            AppScreen::Streaming => {
                self.streaming_menu_open = false;
            }
            AppScreen::AddHost | AppScreen::Connecting | AppScreen::Settings => {}
        }

        self.current_screen = screen;
    }

    /// Adds a new host to the list.
    pub fn add_host(&mut self, mut host: HostEntry) {
        // Check if we're editing an existing host
        if let Some(index) = self.editing_host_index {
            if index < self.hosts.len() {
                // Preserve pairing status and last connected time from existing host
                let existing = &self.hosts[index];
                host.paired = existing.paired;
                host.last_connected = existing.last_connected;

                self.hosts[index] = host;
            }
        } else {
            self.hosts.push(host);
        }

        self.editing_host_index = None;
        self.navigate_to(AppScreen::HostList);
    }

    /// Starts editing an existing host.
    pub fn edit_host(&mut self, index: usize) {
        self.editing_host_index = Some(index);
        self.navigate_to(AppScreen::AddHost);
    }

    /// Deletes a host from the list.
    pub fn delete_host(&mut self, index: usize) {
        if index < self.hosts.len() {
            self.hosts.remove(index);
        }
    }

    /// Gets the host being edited, if any.
    #[must_use]
    pub fn editing_host(&self) -> Option<&HostEntry> {
        self.editing_host_index
            .and_then(|index| self.hosts.get(index))
    }

    /// Connects to a host at the given index.
    pub fn connect_to_host(&mut self, index: usize) {
        if let Some(host) = self.hosts.get(index) {
            self.target_host_name = host.name.clone();

            let action = UiAction::Connect {
                host: host.socket_addr(),
                port: host.port,
                needs_pairing: !host.paired,
            };

            self.send_action(action);

            // Navigate to appropriate screen
            if host.paired {
                self.navigate_to(AppScreen::Connecting);
            } else {
                self.navigate_to(AppScreen::Pairing);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;

    fn create_test_app() -> UiApp {
        let (_event_tx, event_rx) = unbounded();
        let (action_tx, _action_rx) = unbounded();
        UiApp::new(event_rx, action_tx)
    }

    #[test]
    fn test_new_app_starts_on_host_list() {
        let app = create_test_app();
        assert_eq!(app.current_screen, AppScreen::HostList);
    }

    #[test]
    fn test_new_app_has_sample_hosts() {
        let app = create_test_app();
        assert!(!app.hosts.is_empty());
        assert!(app.hosts.iter().any(|h| h.name == "Gaming PC"));
    }

    #[test]
    fn test_navigate_to_clears_state() {
        let mut app = create_test_app();
        app.pin_input = "123456".to_string();
        app.pairing_status = "Error".to_string();

        app.navigate_to(AppScreen::Pairing);

        assert!(app.pin_input.is_empty());
        assert!(app.pairing_status.is_empty());
    }

    #[test]
    fn test_wants_input_streaming_without_menu() {
        let mut app = create_test_app();
        app.current_screen = AppScreen::Streaming;
        app.streaming_menu_open = false;
        app.reconnecting = false;

        assert!(!app.wants_input());
    }

    #[test]
    fn test_wants_input_streaming_with_menu() {
        let mut app = create_test_app();
        app.current_screen = AppScreen::Streaming;
        app.streaming_menu_open = true;

        assert!(app.wants_input());
    }

    #[test]
    fn test_wants_input_streaming_while_reconnecting() {
        let mut app = create_test_app();
        app.current_screen = AppScreen::Streaming;
        app.streaming_menu_open = false;
        app.reconnecting = true;

        assert!(app.wants_input());
    }

    #[test]
    fn test_wants_input_other_screens() {
        let mut app = create_test_app();

        for screen in [
            AppScreen::HostList,
            AppScreen::AddHost,
            AppScreen::Pairing,
            AppScreen::Connecting,
            AppScreen::Settings,
        ] {
            app.current_screen = screen;
            assert!(app.wants_input());
        }
    }

    #[test]
    fn test_pin_required_event_switches_screen() {
        let mut app = create_test_app();

        app.handle_event(UiEvent::PinRequired {
            host_name: "Test Host".to_string(),
        });

        assert_eq!(app.current_screen, AppScreen::Pairing);
        assert_eq!(app.target_host_name, "Test Host");
        assert!(app.pin_input.is_empty());
    }

    #[test]
    fn test_connected_event_switches_screen() {
        let mut app = create_test_app();

        app.handle_event(UiEvent::Connected {
            host_name: "Gaming PC".to_string(),
        });

        assert_eq!(app.current_screen, AppScreen::Streaming);
        assert_eq!(app.target_host_name, "Gaming PC");
        assert!(!app.reconnecting);
    }

    #[test]
    fn test_disconnected_event_returns_to_host_list() {
        let mut app = create_test_app();
        app.current_screen = AppScreen::Streaming;
        app.streaming_menu_open = true;
        app.fullscreen = true;

        app.handle_event(UiEvent::Disconnected { error: None });

        assert_eq!(app.current_screen, AppScreen::HostList);
        assert!(!app.streaming_menu_open);
        assert!(!app.fullscreen);
    }

    #[test]
    fn test_add_host_appends_new_host() {
        let mut app = create_test_app();
        let initial_count = app.hosts.len();

        let new_host = HostEntry::new(
            "Test Host".to_string(),
            std::net::IpAddr::V4(std::net::Ipv4Addr::new(192, 168, 1, 200)),
            7860,
        );

        app.add_host(new_host);

        assert_eq!(app.hosts.len(), initial_count + 1);
        assert_eq!(app.current_screen, AppScreen::HostList);
    }

    #[test]
    fn test_edit_host_preserves_pairing_status() {
        let mut app = create_test_app();

        // Find a paired host
        let paired_index = app.hosts.iter().position(|h| h.paired).unwrap();
        let original_paired = app.hosts[paired_index].paired;
        let original_last_connected = app.hosts[paired_index].last_connected;

        // Start editing
        app.edit_host(paired_index);

        // Create modified host
        let mut modified_host = app.hosts[paired_index].clone();
        modified_host.name = "Modified Name".to_string();

        app.add_host(modified_host);

        // Verify pairing status is preserved
        assert_eq!(app.hosts[paired_index].paired, original_paired);
        assert_eq!(
            app.hosts[paired_index].last_connected,
            original_last_connected
        );
        assert_eq!(app.hosts[paired_index].name, "Modified Name");
    }

    #[test]
    fn test_delete_host_removes_from_list() {
        let mut app = create_test_app();
        let initial_count = app.hosts.len();

        app.delete_host(0);

        assert_eq!(app.hosts.len(), initial_count - 1);
    }

    #[test]
    fn test_delete_host_out_of_bounds_no_panic() {
        let mut app = create_test_app();
        let count = app.hosts.len();
        app.delete_host(999);
        assert_eq!(app.hosts.len(), count);
    }

    #[test]
    fn test_reconnecting_event_sets_state() {
        let mut app = create_test_app();
        app.current_screen = AppScreen::Streaming;

        app.handle_event(UiEvent::Reconnecting {
            host_name: "Host".to_string(),
            countdown_secs: 15,
        });

        assert!(app.reconnecting);
        assert_eq!(app.reconnect_countdown, 15);
    }

    #[test]
    fn test_stream_stats_event() {
        let mut app = create_test_app();
        assert!(app.stream_stats.is_none());

        app.handle_event(UiEvent::StreamStats {
            resolution: "1920x1080".to_string(),
            fps: 60,
            codec: "HEVC".to_string(),
            latency_ms: 2,
        });

        let stats = app.stream_stats.as_ref().expect("stats set");
        assert_eq!(stats.fps, 60);
        assert_eq!(stats.codec, "HEVC");
    }

    #[test]
    fn test_pairing_success_marks_host_paired() {
        let mut app = create_test_app();
        app.target_host_name = "Gaming PC".to_string();

        app.handle_event(UiEvent::PairingResult {
            success: true,
            error: None,
        });

        let host = app
            .hosts
            .iter()
            .find(|h| h.name == "Gaming PC")
            .expect("host");
        assert!(host.paired);
        assert!(app.pairing_status.contains("successful"));
    }

    #[test]
    fn test_pairing_failure_sets_error_status() {
        let mut app = create_test_app();
        app.handle_event(UiEvent::PairingResult {
            success: false,
            error: Some("Wrong PIN".to_string()),
        });
        assert_eq!(app.pairing_status, "Wrong PIN");
    }

    #[test]
    fn test_pairing_failure_default_message() {
        let mut app = create_test_app();
        app.handle_event(UiEvent::PairingResult {
            success: false,
            error: None,
        });
        assert_eq!(app.pairing_status, "Pairing failed");
    }

    #[test]
    fn test_connect_to_paired_host_goes_to_connecting() {
        let (event_tx, event_rx) = unbounded();
        let (action_tx, action_rx) = unbounded();
        let _ = event_tx;
        let mut app = UiApp::new(event_rx, action_tx);

        let paired_idx = app
            .hosts
            .iter()
            .position(|h| h.paired)
            .expect("paired host");
        app.connect_to_host(paired_idx);

        assert_eq!(app.current_screen, AppScreen::Connecting);
        let action = action_rx.try_recv().expect("action sent");
        assert!(matches!(
            action,
            UiAction::Connect {
                needs_pairing: false,
                ..
            }
        ));
    }

    #[test]
    fn test_connect_to_unpaired_host_goes_to_pairing() {
        let (event_tx, event_rx) = unbounded();
        let (action_tx, action_rx) = unbounded();
        let _ = event_tx;
        let mut app = UiApp::new(event_rx, action_tx);

        let unpaired_idx = app
            .hosts
            .iter()
            .position(|h| !h.paired)
            .expect("unpaired host");
        app.connect_to_host(unpaired_idx);

        assert_eq!(app.current_screen, AppScreen::Pairing);
        let action = action_rx.try_recv().expect("action sent");
        assert!(matches!(
            action,
            UiAction::Connect {
                needs_pairing: true,
                ..
            }
        ));
    }

    #[test]
    fn test_connect_to_invalid_index_no_panic() {
        let mut app = create_test_app();
        app.connect_to_host(999); // should not panic
        assert_eq!(app.current_screen, AppScreen::HostList);
    }

    #[test]
    fn test_connected_event_marks_host_connected() {
        let mut app = create_test_app();
        app.handle_event(UiEvent::Connected {
            host_name: "Gaming PC".to_string(),
        });
        let host = app
            .hosts
            .iter()
            .find(|h| h.name == "Gaming PC")
            .expect("host");
        assert!(host.last_connected.is_some());
    }

    #[test]
    fn test_editing_host_returns_correct_host() {
        let mut app = create_test_app();
        assert!(app.editing_host().is_none());

        app.edit_host(0);
        assert!(app.editing_host().is_some());
        assert_eq!(app.editing_host().expect("editing").name, app.hosts[0].name);
    }

    #[test]
    fn test_process_events_drains_channel() {
        let (event_tx, event_rx) = unbounded();
        let (action_tx, _action_rx) = unbounded();
        let mut app = UiApp::new(event_rx, action_tx);

        event_tx
            .send(UiEvent::Disconnected { error: None })
            .expect("send");
        event_tx
            .send(UiEvent::Disconnected { error: None })
            .expect("send");

        app.current_screen = AppScreen::Streaming;
        app.process_events();

        assert_eq!(app.current_screen, AppScreen::HostList);
        // Channel drained — app consumed both events
    }

    #[test]
    fn test_navigate_to_host_list_clears_editing_index() {
        let mut app = create_test_app();
        app.editing_host_index = Some(0);
        app.navigate_to(AppScreen::HostList);
        assert!(app.editing_host_index.is_none());
    }

    #[test]
    fn test_navigate_to_streaming_closes_menu() {
        let mut app = create_test_app();
        app.streaming_menu_open = true;
        app.navigate_to(AppScreen::Streaming);
        assert!(!app.streaming_menu_open);
    }

    #[test]
    fn test_send_action_with_closed_channel_no_panic() {
        let (_event_tx, event_rx) = unbounded();
        let (action_tx, action_rx) = unbounded();
        drop(action_rx); // close receiver
        let app = UiApp::new(event_rx, action_tx);
        // Should log warning but not panic
        app.send_action(UiAction::Disconnect);
    }
}
