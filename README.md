# 🎮 RayPlay

Low-latency game streaming from Windows to macOS, built entirely in Rust.

**RayHost** (server) captures your screen on a Windows PC with an Nvidia GPU, encodes
it via NVENC, and streams it over the network. **RayView** (client) runs on macOS,
decodes the stream, renders frames, and relays keyboard/mouse input back to the host
with near-zero latency.

## ✨ Features

- **Extreme low latency** — sub-16ms glass-to-glass target at 60fps
- **Exclusive input mode** — mouse and keyboard fully trapped in client window,
  relayed directly to host (USB-passthrough-like experience)
- **Hardware encoding** — Nvidia NVENC for near-instant frame encoding
- **Cross-platform** — Windows server, macOS client (Android planned)
- **Modular architecture** — cargo workspace with independent, reusable crates

## 📦 Project Structure

    rayplay/
    ├── Cargo.toml                  # Workspace root
    ├── CLAUDE.md                   # AI agent project context
    ├── Makefile.toml               # cargo-make build tasks
    ├── crates/
    │   ├── rayplay-core/           # Core streaming logic, shared types, config
    │   ├── rayplay-network/        # Networking (UDP relay, WebRTC, discovery)
    │   ├── rayplay-video/          # Video capture, NVENC encoding, decoding
    │   ├── rayplay-input/          # Input capture, relay, exclusive mode
    │   └── rayplay-cli/            # CLI interface (clap) for server and client
    ├── docs/
    │   ├── requirements/           # Functional requirements & roadmap
    │   ├── uc/                     # Use Case documents (UC-001 … UC-025)
    │   └── adr/                    # Architecture Decision Records (ADR-000 … ADR-008)
    └── .github/
        └── workflows/              # CI/CD pipelines

## 🚀 Quick Start

### Prerequisites

- Rust toolchain (stable): https://rustup.rs
- cargo-make: `cargo install cargo-make`
- cargo-nextest: `cargo install cargo-nextest`
- cargo-llvm-cov: `cargo install cargo-llvm-cov`

### Build

    git clone https://github.com/hakanserce/rayplay.git
    cd rayplay
    cargo make build

### Run

    # Start the server (Windows host)
    cargo run -p rayplay-cli -- server --bind 0.0.0.0:9000

    # Start the client (macOS)
    cargo run -p rayplay-cli -- client --host 192.168.1.100:9000

### Test

    cargo make test              # Run all tests (nextest)
    cargo make lint              # clippy --pedantic
    cargo make coverage          # Coverage report
    cargo make ci                # Full CI pipeline (fmt + lint + test + coverage)

See [DEVELOPMENT.md](DEVELOPMENT.md) for the complete development guide.

## 🏗️ Architecture

RayPlay is organized as a Cargo workspace with five crates. The full design is captured
in [ADR-000: High Level Architecture Design](docs/adr/ADR-000.md).

### System Overview

```
┌─────────────────────────────────────┐     ┌─────────────────────────────────────┐
│          RayHost (Windows)          │     │          RayView (macOS/Android)    │
│                                     │     │                                     │
│  Screen  →  Capture  →  Encode  ─────────▶  Decode  →  Render  →  Display     │
│                                     │     │                                     │
│  ◀──── Input Injection              │     │  Input Capture ────▶                │
│                                     │     │                                     │
│  Audio Capture  →  Encode  ──────────────▶  Decode  →  Playback                │
│                                     │     │                                     │
│  Discovery / Session / Security ◀────────▶  Discovery / Session / Security     │
└─────────────────────────────────────┘     └─────────────────────────────────────┘
```

### Crate Responsibilities

| Crate              | Type    | Purpose                                          |
| ------------------ | ------- | ------------------------------------------------ |
| `rayplay-core`     | Library | Shared types: `Frame`, `Packet`, `SessionConfig`, error traits |
| `rayplay-network`  | Library | Transport (QUIC/UDP), framing, reassembly, mDNS discovery |
| `rayplay-video`    | Library | Capture (DXGI), NVENC encoding, VideoToolbox decoding, render |
| `rayplay-input`    | Library | Input capture (winit/CGEvent), injection (SendInput), serialisation |
| `rayplay-cli`      | Binary  | Entry points for RayHost and RayView; CLI parsing via clap |

Library crates use `thiserror` for error handling. The CLI crate uses `anyhow`.

### Performance Budget

| Stage                | Target   | Notes                               |
| -------------------- | -------- | ----------------------------------- |
| Screen capture       | < 1 ms   | DXGI Desktop Duplication, GPU texture |
| NVENC encode         | < 5 ms   | Hardware, zero-copy input           |
| Network (LAN)        | < 1 ms   | Transport overhead only             |
| VideoToolbox decode  | < 3 ms   | Hardware, Apple Silicon             |
| Render / present     | < 2 ms   | Metal / wgpu, direct texture present |
| **Total**            | **< 16 ms** | Sub-frame at 60 fps              |
| Input round-trip     | < 2 ms   | Client capture → host inject        |

## 🔧 Key Dependencies

| Crate         | Purpose                          |
| ------------- | -------------------------------- |
| `tokio`       | Async runtime                    |
| `axum`        | Web framework (signaling, API)   |
| `serde`       | Serialization/deserialization    |
| `tracing`     | Structured logging               |
| `clap`        | CLI argument parsing             |
| `anyhow`      | Application error handling       |
| `thiserror`   | Library error types              |
| `criterion`   | Benchmarking                     |

## 📋 Roadmap

The full feature roadmap is tracked in [docs/requirements/product-roadmap.md](docs/requirements/product-roadmap.md).

| Milestone | Goal | Key UCs |
| --------- | ---- | ------- |
| **M1 — Minimum Viable Stream** | See host screen on client at 60 fps | [UC-001](docs/uc/UC-001.md) → [UC-008](docs/uc/UC-008.md) |
| **M2 — Usable Product** | Input, audio, auto-discovery, security | [UC-009](docs/uc/UC-009.md) → [UC-016](docs/uc/UC-016.md), [UC-025](docs/uc/UC-025.md) |
| **M3 — Polish** | Adaptive quality, multi-monitor, gamepad, Android | [UC-017](docs/uc/UC-017.md) → [UC-024](docs/uc/UC-024.md) |

## 📐 Architecture Decision Records

Significant design decisions are documented in [`docs/adr/`](docs/adr/).

| ADR | Title | Status |
| --- | ----- | ------ |
| [ADR-000](docs/adr/ADR-000.md) | High Level Architecture Design | Proposed |
| [ADR-001](docs/adr/ADR-001.md) | Zero-Copy Graphics Capture/Encode/Send | Proposed |
| [ADR-002](docs/adr/ADR-002.md) | Low-Latency Audio Capture/Encode/Send | Proposed |
| [ADR-003](docs/adr/ADR-003.md) | Streaming Protocol Selection | Proposed |
| [ADR-004](docs/adr/ADR-004.md) | Video Codec FFI Approach (NVENC / VideoToolbox) | Proposed |
| [ADR-005](docs/adr/ADR-005.md) | Window and Rendering Framework | Proposed |
| [ADR-006](docs/adr/ADR-006.md) | Input Capture Mechanism Per Platform | Proposed |
| [ADR-007](docs/adr/ADR-007.md) | Security Model (Encryption, Authentication, Trust) | Proposed |
| [ADR-008](docs/adr/ADR-008.md) | Android Client UX Design and Platform Trade-offs | Proposed |

## 📁 Use Cases

All features are specified as Use Cases in [`docs/uc/`](docs/uc/).

<details>
<summary>Milestone 1 — Minimum Viable Stream</summary>

| UC | Title |
| -- | ----- |
| [UC-001](docs/uc/UC-001.md) | Host Screen Capture |
| [UC-002](docs/uc/UC-002.md) | Host Video Encoding |
| [UC-003](docs/uc/UC-003.md) | Video Stream Transport |
| [UC-004](docs/uc/UC-004.md) | Client Video Decoding |
| [UC-005](docs/uc/UC-005.md) | Client Frame Rendering |
| [UC-006](docs/uc/UC-006.md) | Host CLI (RayHost) |
| [UC-007](docs/uc/UC-007.md) | Client CLI (RayView) |
| [UC-008](docs/uc/UC-008.md) | End-to-End Video Streaming |

</details>

<details>
<summary>Milestone 2 — Usable Product</summary>

| UC | Title |
| -- | ----- |
| [UC-009](docs/uc/UC-009.md) | Client Keyboard Input Relay |
| [UC-010](docs/uc/UC-010.md) | Client Mouse Input Relay |
| [UC-011](docs/uc/UC-011.md) | Exclusive Input Mode |
| [UC-012](docs/uc/UC-012.md) | Host Audio Streaming |
| [UC-013](docs/uc/UC-013.md) | Client Audio Playback |
| [UC-014](docs/uc/UC-014.md) | Automatic Host Discovery |
| [UC-015](docs/uc/UC-015.md) | Session Management |
| [UC-016](docs/uc/UC-016.md) | Connection Pairing & Security |
| [UC-025](docs/uc/UC-025.md) | Host Wake-on-LAN |

</details>

<details>
<summary>Milestone 3 — Polish</summary>

| UC | Title |
| -- | ----- |
| [UC-017](docs/uc/UC-017.md) | Latency Metrics & Overlay |
| [UC-018](docs/uc/UC-018.md) | Adaptive Stream Quality |
| [UC-019](docs/uc/UC-019.md) | Multi-Monitor Selection |
| [UC-020](docs/uc/UC-020.md) | Gamepad Input Support |
| [UC-021](docs/uc/UC-021.md) | Client-Side Cursor Rendering |
| [UC-022](docs/uc/UC-022.md) | Android Client |
| [UC-023](docs/uc/UC-023.md) | Additional Codec Support |
| [UC-024](docs/uc/UC-024.md) | Configuration Persistence |

</details>

## 📄 Documentation

| Document | Description |
| -------- | ----------- |
| [DEVELOPMENT.md](DEVELOPMENT.md) | Development setup, standards, quality gates, and workflow |
| [EncodingConcepts.md](EncodingConcepts.md) | Video encoding background: NVENC, H.264/H.265, zero-copy |
| [RayPlayNetworking.md](RayPlayNetworking.md) | Networking deep-dive: UDP, QUIC, WebRTC, packet framing |
| [Daily_Prompts_Slash_Commands_Reference.md](Daily_Prompts_Slash_Commands_Reference.md) | Agent slash commands and daily workflow prompts |
| [CLAUDE.md](CLAUDE.md) | AI agent project context (read by Claude Code automatically) |
| [docs/requirements/product-roadmap.md](docs/requirements/product-roadmap.md) | Full product roadmap with milestones and UC dependency graph |
| [`docs/uc/`](docs/uc/) | Use Case specifications (UC-001 … UC-025) |
| [`docs/adr/`](docs/adr/) | Architecture Decision Records (ADR-000 … ADR-008) |

## 🤝 Contributing

This project uses an agentic development workflow with Claude Code. See
[DEVELOPMENT.md](DEVELOPMENT.md) for standards, quality gates, and PR process.

All changes go through pull requests targeting `main`. Every PR must:
- Pass `cargo make ci` (fmt + clippy + tests + coverage)
- Reference a UC ID
- Be reviewed before merge

## 📝 License

TBD
