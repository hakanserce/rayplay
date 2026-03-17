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
| [UC-001](../uc/UC-001.md) | Host Screen Capture | P0 | M |
| [UC-002](../uc/UC-002.md) | Host Video Encoding | P0 | L |
| [UC-003](../uc/UC-003.md) | Video Stream Transport | P0 | L |
| [UC-004](../uc/UC-004.md) | Client Video Decoding | P0 | L |
| [UC-005](../uc/UC-005.md) | Client Frame Rendering | P0 | M |
| [UC-006](../uc/UC-006.md) | Host CLI (RayHost) | P0 | S |
| [UC-007](../uc/UC-007.md) | Client CLI (RayView) | P0 | S |
| [UC-008](../uc/UC-008.md) | End-to-End Video Streaming | P0 | M |
| [UC-026](../uc/UC-026.md) | Platform-Neutral Fallback Pipeline | P1 | L |

---

## Milestone 2 — Usable Product

**Goal:** Full interactive gaming — keyboard/mouse control, game audio, automatic
server discovery on LAN, and PIN-based security.

| UC | Title | Priority | Size |
|----|-------|----------|------|
| [UC-009](../uc/UC-009.md) | Client Keyboard Input Relay | P0 | M |
| [UC-010](../uc/UC-010.md) | Client Mouse Input Relay | P0 | M |
| [UC-011](../uc/UC-011.md) | Exclusive Input Mode | P0 | M |
| [UC-012](../uc/UC-012.md) | Host Audio Streaming | P1 | M |
| [UC-013](../uc/UC-013.md) | Client Audio Playback | P1 | M |
| [UC-014](../uc/UC-014.md) | Automatic Host Discovery | P1 | M |
| [UC-015](../uc/UC-015.md) | Session Management | P1 | M |
| [UC-016](../uc/UC-016.md) | Connection Pairing & Security | P1 | M |
| [UC-025](../uc/UC-025.md) | Host Wake-on-LAN | P1 | M |

---

## Milestone 3 — Polish

**Goal:** Production-quality features for real-world conditions — adaptive quality,
multi-monitor, gamepad, Android client, and codec flexibility.

| UC | Title | Priority | Size |
|----|-------|----------|------|
| [UC-017](../uc/UC-017.md) | Latency Metrics & Overlay | P1 | M |
| [UC-018](../uc/UC-018.md) | Adaptive Stream Quality | P2 | L |
| [UC-019](../uc/UC-019.md) | Multi-Monitor Selection | P2 | M |
| [UC-020](../uc/UC-020.md) | Gamepad Input Support | P2 | M |
| [UC-021](../uc/UC-021.md) | Client-Side Cursor Rendering | P2 | M |
| [UC-022](../uc/UC-022.md) | Android Client | P1 | L |
| [UC-023](../uc/UC-023.md) | Additional Codec Support | P2 | M |
| [UC-024](../uc/UC-024.md) | Configuration Persistence | P2 | S |

---

## Architecture Decision Records (Prerequisites)

The following ADRs must be resolved before implementing the corresponding UCs:

| ADR | Title | Blocks |
|-----|-------|--------|
| [ADR-000](../adr/ADR-000.md) | High level architecture design | All UCs |
| [ADR-001](../adr/ADR-001.md) | Zero-copy graphics capture/encode/send architecture | [UC-001](../uc/UC-001.md), [UC-002](../uc/UC-002.md), [UC-003](../uc/UC-003.md) |
| [ADR-002](../adr/ADR-002.md) | Low-latency audio capture/encode/send architecture | [UC-012](../uc/UC-012.md), [UC-013](../uc/UC-013.md) |
| [ADR-003](../adr/ADR-003.md) | Streaming protocol (raw UDP vs QUIC vs WebRTC vs other) | [UC-003](../uc/UC-003.md), [UC-009](../uc/UC-009.md), [UC-010](../uc/UC-010.md), [UC-012](../uc/UC-012.md) |
| [ADR-004](../adr/ADR-004.md) | Video codec FFI approach (NVENC/VideoToolbox bindings) | [UC-002](../uc/UC-002.md), [UC-004](../uc/UC-004.md) |
| [ADR-005](../adr/ADR-005.md) | Window/rendering framework | [UC-005](../uc/UC-005.md), [UC-022](../uc/UC-022.md) |
| [ADR-006](../adr/ADR-006.md) | Input capture mechanism per platform | [UC-009](../uc/UC-009.md), [UC-010](../uc/UC-010.md), [UC-011](../uc/UC-011.md) |
| [ADR-007](../adr/ADR-007.md) | Security model (encryption, authentication, trust) | [UC-016](../uc/UC-016.md) |
| [ADR-008](../adr/ADR-008.md) | Android client UX design and platform trade-offs | [UC-022](../uc/UC-022.md) |
| [ADR-009](../adr/ADR-009.md) | Platform-neutral fallback pipeline strategy | [UC-026](../uc/UC-026.md) |

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
  Connect: UC-014, UC-015 → UC-016 → UC-025

M3 (mostly independent):
  UC-017 → UC-018
  UC-019 through UC-024 (flexible order)
```
