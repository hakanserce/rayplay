# RayPlay Product Roadmap

## Vision

RayPlay streams games from a Windows host to a macOS client with extreme low latency.
The client should feel native — exclusive input mode where the mouse is fully trapped
in the client window and all input goes directly to the host.

## Performance Targets

| Metric | Target |
|--------|--------|
| Video encoding | <5 ms per frame |
| Network round-trip overhead | <1 ms |
| Input relay end-to-end | <2 ms |
| Glass-to-glass total | <16 ms (sub-frame at 60 fps) |

---

## Milestone 1 — Minimum Viable Stream

**Goal:** A user starts RayHost on Windows and RayView on macOS and sees the host's
screen rendered on the client at 60 fps. Video only, manual IP connection, CLI interface.

| UC | Title | Priority | Size |
|----|-------|----------|------|
| UC-001 | Host Screen Capture | P0 | M |
| UC-002 | Host Video Encoding | P0 | L |
| UC-003 | Video Stream Transport | P0 | L |
| UC-004 | Client Video Decoding | P0 | L |
| UC-005 | Client Frame Rendering | P0 | M |
| UC-006 | Host CLI (RayHost) | P0 | S |
| UC-007 | Client CLI (RayView) | P0 | S |
| UC-008 | End-to-End Video Streaming | P0 | M |

---

## Milestone 2 — Usable Product

**Goal:** Full interactive gaming — keyboard/mouse control, game audio, automatic
server discovery on LAN, and PIN-based security.

| UC | Title | Priority | Size |
|----|-------|----------|------|
| UC-009 | Client Keyboard Input Relay | P0 | M |
| UC-010 | Client Mouse Input Relay | P0 | M |
| UC-011 | Exclusive Input Mode | P0 | M |
| UC-012 | Host Audio Streaming | P1 | M |
| UC-013 | Client Audio Playback | P1 | M |
| UC-014 | Automatic Host Discovery | P1 | M |
| UC-015 | Session Management | P1 | M |
| UC-016 | Connection Pairing & Security | P1 | M |

---

## Milestone 3 — Polish

**Goal:** Production-quality features for real-world conditions — adaptive quality,
multi-monitor, gamepad, Android client, and codec flexibility.

| UC | Title | Priority | Size |
|----|-------|----------|------|
| UC-017 | Latency Metrics & Overlay | P1 | M |
| UC-018 | Adaptive Stream Quality | P2 | L |
| UC-019 | Multi-Monitor Selection | P2 | M |
| UC-020 | Gamepad Input Support | P2 | M |
| UC-021 | Client-Side Cursor Rendering | P2 | M |
| UC-022 | Android Client | P1 | L |
| UC-023 | Additional Codec Support | P2 | M |
| UC-024 | Configuration Persistence | P2 | S |

---

## Architecture Decision Records (Prerequisites)

The following ADRs must be resolved before implementing the corresponding UCs:

| ADR | Title | Blocks |
|-----|-------|--------|
| ADR-001 | Zero-copy graphics capture/encode/send architecture | UC-001, UC-002, UC-003 |
| ADR-002 | Low-latency audio capture/encode/send architecture | UC-012, UC-013 |
| ADR-003 | Streaming protocol (raw UDP vs QUIC vs WebRTC vs other) | UC-003, UC-009, UC-010, UC-012 |
| ADR-004 | Video codec FFI approach (NVENC/VideoToolbox bindings) | UC-002, UC-004 |
| ADR-005 | Window/rendering framework | UC-005, UC-022 |
| ADR-006 | Input capture mechanism per platform | UC-009, UC-010, UC-011 |
| ADR-007 | Security model (encryption, authentication, trust) | UC-016 |

---

## Dependency Graph

```
M1 (serial pipeline):
  UC-001 → UC-002 → UC-003 → UC-004 → UC-005
  UC-006 ← (UC-002 + UC-003)
  UC-007 ← (UC-004 + UC-005)
  UC-008 ← UC-006 + UC-007

M2 (three parallel tracks after UC-008):
  Input:   UC-009 + UC-010 → UC-011
  Audio:   UC-012 → UC-013
  Connect: UC-014, UC-015 → UC-016

M3 (mostly independent):
  UC-017 → UC-018
  UC-019 through UC-024 (flexible order)
```
