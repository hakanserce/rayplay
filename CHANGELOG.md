# Changelog

All notable changes to RayPlay will be documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- NVENC hardware encoder for Windows (UC-102)
- VtDecoder FFI on macOS with H.264 keyframe detection
- End-to-end video streaming pipeline (UC-008)
- Client video decoding via VideoToolbox (UC-004)
- Client frame rendering via Metal/wgpu (UC-005)
- Video stream transport over UDP (UC-003)
- Host NVENC video encoding (UC-002)
- Host screen capture via DXGI (UC-001)
- Host CLI (`rayhost`) with clap (UC-006)
- Client CLI (`rayview`) with clap (UC-007)
- Connection pairing via SPAKE2 PIN exchange (UC-016)
- Input relay for keyboard and mouse (UC-009, UC-010)
- Cross-platform CI pipeline (Linux, macOS, Windows)
- 99% code coverage gate

[Unreleased]: https://github.com/hakanserce/rayplay/commits/main
