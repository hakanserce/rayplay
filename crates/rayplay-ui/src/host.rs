//! Host entry management for the UI.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

/// A host entry in the host list.
///
/// Contains all information needed to display and connect to a `RayPlay` host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostEntry {
    /// Display name for the host.
    pub name: String,
    /// IP address or hostname.
    pub address: IpAddr,
    /// Port number (typically 7860).
    pub port: u16,
    /// Whether this host has been paired with this client.
    pub paired: bool,
    /// Whether this host was discovered via mDNS.
    pub discovered: bool,
    /// Timestamp of the last successful connection, if any.
    pub last_connected: Option<DateTime<Utc>>,
}

impl HostEntry {
    /// Creates a new host entry with default values.
    #[must_use]
    pub fn new(name: String, address: IpAddr, port: u16) -> Self {
        Self {
            name,
            address,
            port,
            paired: false,
            discovered: false,
            last_connected: None,
        }
    }

    /// Returns a socket address for connecting to this host.
    #[must_use]
    pub fn socket_addr(&self) -> std::net::SocketAddr {
        std::net::SocketAddr::new(self.address, self.port)
    }

    /// Updates the last connected timestamp to now.
    pub fn mark_connected(&mut self) {
        self.last_connected = Some(Utc::now());
    }

    /// Marks this host as paired.
    pub fn mark_paired(&mut self) {
        self.paired = true;
    }

    /// Returns a human-readable string for the last connected time.
    #[must_use]
    pub fn last_connected_display(&self) -> String {
        match self.last_connected {
            None => "Never connected".to_string(),
            Some(timestamp) => {
                let now = Utc::now();
                let duration = now - timestamp;

                let hours = duration.num_hours();
                if hours < 1 {
                    "Just now".to_string()
                } else if hours < 24 {
                    format!("{hours} hours ago")
                } else {
                    let days = duration.num_days();
                    if days == 1 {
                        "1 day ago".to_string()
                    } else {
                        format!("{days} days ago")
                    }
                }
            }
        }
    }

    /// Returns the status badges to display for this host.
    #[must_use]
    pub fn badges(&self) -> Vec<HostBadge> {
        let mut badges = Vec::new();

        if self.paired {
            badges.push(HostBadge::Paired);
        } else {
            badges.push(HostBadge::Unpaired);
        }

        if self.discovered {
            badges.push(HostBadge::Discovered);
        }

        badges
    }
}

/// Visual badges shown on host cards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostBadge {
    /// Host is paired with this client.
    Paired,
    /// Host is not paired with this client.
    Unpaired,
    /// Host was discovered via mDNS.
    Discovered,
}

impl HostBadge {
    /// Returns the display text for this badge.
    #[must_use]
    pub fn text(self) -> &'static str {
        match self {
            Self::Paired => "Paired",
            Self::Unpaired => "Unpaired",
            Self::Discovered => "Discovered",
        }
    }

    /// Returns the egui color for this badge.
    #[must_use]
    pub fn color(self) -> egui::Color32 {
        match self {
            Self::Paired => egui::Color32::from_rgb(74, 144, 226), // Blue
            Self::Unpaired => egui::Color32::from_rgb(153, 153, 153), // Gray
            Self::Discovered => egui::Color32::from_rgb(80, 200, 120), // Green
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn sample_host() -> HostEntry {
        HostEntry::new(
            "Test PC".to_string(),
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 42)),
            7860,
        )
    }

    // ── HostEntry::new ────────────────────────────────────────────────

    #[test]
    fn test_new_host_has_correct_fields() {
        let host = sample_host();
        assert_eq!(host.name, "Test PC");
        assert_eq!(host.address, IpAddr::V4(Ipv4Addr::new(192, 168, 1, 42)));
        assert_eq!(host.port, 7860);
    }

    #[test]
    fn test_new_host_defaults_unpaired() {
        assert!(!sample_host().paired);
    }

    #[test]
    fn test_new_host_defaults_not_discovered() {
        assert!(!sample_host().discovered);
    }

    #[test]
    fn test_new_host_defaults_never_connected() {
        assert!(sample_host().last_connected.is_none());
    }

    // ── socket_addr ───────────────────────────────────────────────────

    #[test]
    fn test_socket_addr_combines_ip_and_port() {
        let host = sample_host();
        let addr = host.socket_addr();
        assert_eq!(addr.ip(), IpAddr::V4(Ipv4Addr::new(192, 168, 1, 42)));
        assert_eq!(addr.port(), 7860);
    }

    #[test]
    fn test_socket_addr_ipv6() {
        let host = HostEntry::new(
            "IPv6 Host".to_string(),
            IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
            9000,
        );
        assert_eq!(host.socket_addr().port(), 9000);
        assert!(host.socket_addr().ip().is_loopback());
    }

    // ── mark_connected / mark_paired ──────────────────────────────────

    #[test]
    fn test_mark_connected_sets_timestamp() {
        let mut host = sample_host();
        assert!(host.last_connected.is_none());
        host.mark_connected();
        assert!(host.last_connected.is_some());
    }

    #[test]
    fn test_mark_connected_updates_timestamp() {
        let mut host = sample_host();
        host.mark_connected();
        let first = host.last_connected.expect("should be set");
        // Call again — should update (or at least not regress)
        host.mark_connected();
        let second = host.last_connected.expect("should be set");
        assert!(second >= first);
    }

    #[test]
    fn test_mark_paired_sets_flag() {
        let mut host = sample_host();
        assert!(!host.paired);
        host.mark_paired();
        assert!(host.paired);
    }

    #[test]
    fn test_mark_paired_idempotent() {
        let mut host = sample_host();
        host.mark_paired();
        host.mark_paired();
        assert!(host.paired);
    }

    // ── last_connected_display ────────────────────────────────────────

    #[test]
    fn test_display_never_connected() {
        assert_eq!(sample_host().last_connected_display(), "Never connected");
    }

    #[test]
    fn test_display_just_now() {
        let mut host = sample_host();
        host.last_connected = Some(Utc::now());
        assert_eq!(host.last_connected_display(), "Just now");
    }

    #[test]
    fn test_display_hours_ago() {
        let mut host = sample_host();
        host.last_connected = Some(Utc::now() - chrono::Duration::hours(5));
        assert_eq!(host.last_connected_display(), "5 hours ago");
    }

    #[test]
    fn test_display_one_day_ago() {
        let mut host = sample_host();
        host.last_connected = Some(Utc::now() - chrono::Duration::days(1));
        assert_eq!(host.last_connected_display(), "1 day ago");
    }

    #[test]
    fn test_display_multiple_days_ago() {
        let mut host = sample_host();
        host.last_connected = Some(Utc::now() - chrono::Duration::days(7));
        assert_eq!(host.last_connected_display(), "7 days ago");
    }

    #[test]
    fn test_display_boundary_23_hours() {
        let mut host = sample_host();
        host.last_connected = Some(Utc::now() - chrono::Duration::hours(23));
        assert_eq!(host.last_connected_display(), "23 hours ago");
    }

    // ── badges ────────────────────────────────────────────────────────

    #[test]
    fn test_badges_unpaired_no_discovery() {
        let host = sample_host();
        let badges = host.badges();
        assert_eq!(badges, vec![HostBadge::Unpaired]);
    }

    #[test]
    fn test_badges_paired() {
        let mut host = sample_host();
        host.paired = true;
        let badges = host.badges();
        assert_eq!(badges, vec![HostBadge::Paired]);
    }

    #[test]
    fn test_badges_discovered_and_unpaired() {
        let mut host = sample_host();
        host.discovered = true;
        let badges = host.badges();
        assert_eq!(badges, vec![HostBadge::Unpaired, HostBadge::Discovered]);
    }

    #[test]
    fn test_badges_paired_and_discovered() {
        let mut host = sample_host();
        host.paired = true;
        host.discovered = true;
        let badges = host.badges();
        assert_eq!(badges, vec![HostBadge::Paired, HostBadge::Discovered]);
    }

    // ── HostBadge ─────────────────────────────────────────────────────

    #[test]
    fn test_badge_text() {
        assert_eq!(HostBadge::Paired.text(), "Paired");
        assert_eq!(HostBadge::Unpaired.text(), "Unpaired");
        assert_eq!(HostBadge::Discovered.text(), "Discovered");
    }

    #[test]
    fn test_badge_colors_are_distinct() {
        let paired = HostBadge::Paired.color();
        let unpaired = HostBadge::Unpaired.color();
        let discovered = HostBadge::Discovered.color();
        assert_ne!(paired, unpaired);
        assert_ne!(paired, discovered);
        assert_ne!(unpaired, discovered);
    }

    // ── Serialization round-trip ──────────────────────────────────────

    #[test]
    fn test_host_entry_serialization_roundtrip() {
        let mut host = sample_host();
        host.paired = true;
        host.discovered = true;
        host.mark_connected();

        let json = serde_json::to_string(&host).expect("serialize");
        let deserialized: HostEntry = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(host, deserialized);
    }
}
