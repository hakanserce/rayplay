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
    │   ├── requirements/           # Functional requirements
    │   ├── uc/                     # Use Case documents
    │   └── adr/                    # Architecture Decision Records
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

RayPlay is organized as a Cargo workspace with five crates:

| Crate              | Type    | Purpose                                          |
| ------------------ | ------- | ------------------------------------------------ |
| `rayplay-core`     | Library | Core streaming logic, shared types, configuration |
| `rayplay-network`  | Library | UDP relay, WebRTC signaling, peer discovery       |
| `rayplay-video`    | Library | Screen capture, NVENC encoding, decoding          |
| `rayplay-input`    | Library | Input capture, relay, exclusive mouse mode         |
| `rayplay-cli`      | Binary  | CLI interface for both server and client           |

Library crates use `thiserror` for error handling. The CLI crate uses `anyhow`.

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

## 🎯 Performance Targets

| Metric                    | Target        |
| ------------------------- | ------------- |
| Video encoding (NVENC)    | < 5ms/frame   |
| Network round-trip        | < 1ms         |
| Input relay end-to-end    | < 2ms         |
| Glass-to-glass total      | < 16ms (60fps)|

## 📄 Documentation

- [DEVELOPMENT.md](DEVELOPMENT.md) — Development setup, standards, and workflow
- [CLAUDE.md](CLAUDE.md) — AI agent context (read by Claude Code automatically)
- `docs/uc/` — Use Case specifications
- `docs/adr/` — Architecture Decision Records
- `docs/requirements/` — Functional requirements

## 🤝 Contributing

This project uses an agentic development workflow with Claude Code. See
[DEVELOPMENT.md](DEVELOPMENT.md) for standards, quality gates, and PR process.

All changes go through pull requests targeting `main`. Every PR must:
- Pass `cargo make ci` (fmt + clippy + tests + coverage)
- Reference a UC ID
- Be reviewed before merge

## 📝 License
TBD
