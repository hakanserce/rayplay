# Encoding Concepts

A guide to the types and pipeline stages in `rayplay-video`.

## Pipeline Overview

```
Screen
  │
  │  DXGI Desktop Duplication API (UC-001)
  ▼
RawFrame
  │
  │  VideoEncoder::encode()
  ▼
EncodedPacket
  │
  │  FrameChunker::chunk()
  ▼
Vec<NetworkChunk>
  │
  │  UDP send (rayplay-network)
  ▼
Client
```

---

## Types

### `RawFrame`

An uncompressed screen capture frame straight from the GPU.

- **Format:** BGRA, 8 bits per channel (matches DXGI output)
- **Fields:** `data`, `width`, `height`, `stride`, `timestamp_us`
- **Stride:** bytes per row, may exceed `width * 4` due to GPU alignment padding
- **Produced by:** screen capture (UC-001)
- **Consumed by:** `VideoEncoder::encode()`

```
┌─────────────────────────────── stride ──────────────────────────────────┐
│ B G R A │ B G R A │ ... │ B G R A │ (padding) │  ← row 0               │
│ B G R A │ B G R A │ ... │ B G R A │ (padding) │  ← row 1               │
│ ...                                                                      │
│ B G R A │ B G R A │ ... │ B G R A │ (padding) │  ← row height-1        │
└──────────────────────────────────────────────────────────────────────────┘
 ◄──────── width * 4 bytes ────────►◄─ pad ────►
```

---

### `EncoderConfig`

Parameters for a single encoder session.

- **Fields:** `codec`, `width`, `height`, `fps`, `bitrate`
- **Default codec:** `Codec::Hevc` (H.265)
- **Bitrate:** `Bitrate::Auto` computes a target from resolution and fps;
  `Bitrate::Mbps(n)` fixes it explicitly

Auto-bitrate formula:
```
bps = (width × height × fps) / 20
      clamped to [1 Mbps, 100 Mbps]
```

Examples at the default factor:

| Resolution | FPS | Auto bitrate |
|------------|-----|-------------|
| 1920×1080  |  60 | ~6.2 Mbps   |
| 2560×1440  |  60 | ~11.1 Mbps  |
| 3840×2160  |  60 | ~24.9 Mbps  |
| 3840×2160  | 120 | ~49.8 Mbps  |

---

### `VideoEncoder` (trait)

The interface every hardware or software encoder implements.

```
┌─────────────────────────────────────────┐
│ VideoEncoder                            │
│─────────────────────────────────────────│
│ encode(frame) → Option<EncodedPacket>   │
│ flush()       → Vec<EncodedPacket>      │
│ config()      → &EncoderConfig          │
└─────────────────────────────────────────┘
         ▲
         │ implements
         │
┌────────────────┐
│ NvencEncoder   │  (Windows only — requires Nvidia RTX 2060+)
│────────────────│
│ NVENC session  │
│ DXGI resource  │
└────────────────┘
```

- `encode()` returns `None` while the encoder is filling its internal pipeline,
  then `Some(packet)` once a frame is ready — mirrors NVENC's async model.
- `flush()` drains any buffered frames at end-of-stream.
- `NvencEncoder` is compiled only on Windows (`#[cfg(target_os = "windows")]`).

---

### `EncodedPacket`

One encoded video frame (a set of HEVC NAL units) output by the encoder.

- **Fields:** `data` (raw bitstream bytes), `is_keyframe`, `timestamp_us`, `duration_us`
- **Keyframe (IDR):** self-contained; the decoder can start here without prior frames
- **P-frame:** depends on previously decoded frames; much smaller than a keyframe
- **Produced by:** `VideoEncoder::encode()` / `VideoEncoder::flush()`
- **Consumed by:** `FrameChunker::chunk()`

Typical sizes over HEVC at 1080p60:

```
Keyframe  ████████████████████████████████████  ~150 kB
P-frame   ████                                  ~20 kB
```

---

### `FrameChunker`

Splits an `EncodedPacket` into UDP-sized pieces.

- **Default chunk size:** 1200 bytes (fits within a 1280-byte IPv6 MTU with headers)
- **Produced by:** calling `FrameChunker::chunk(packet)`
- **Output:** `Vec<NetworkChunk>` — reassembled on the client using packet/chunk indices

```
EncodedPacket  (e.g. 3600 bytes)
│
├──► NetworkChunk { packet_index: 7, chunk_index: 0, total_chunks: 3, data: [1200 B] }
├──► NetworkChunk { packet_index: 7, chunk_index: 1, total_chunks: 3, data: [1200 B] }
└──► NetworkChunk { packet_index: 7, chunk_index: 2, total_chunks: 3, data: [1200 B] }
```

The `packet_index` wraps at `u32::MAX`. `total_chunks` is capped at `u16::MAX`
(≈ 78 MB per packet at the default chunk size — unreachable in practice).

---

### `NetworkChunk`

A single UDP datagram payload.

- **Fields:** `data`, `packet_index`, `chunk_index`, `total_chunks`, `is_keyframe`, `timestamp_us`
- **Produced by:** `FrameChunker::chunk()`
- **Consumed by:** the network layer (`rayplay-network`) and reassembled on the client

The receiver buffers chunks by `packet_index` and reassembles when `chunk_index == total_chunks - 1`.

---

## End-to-End Example (1080p60 keyframe)

```
RawFrame
  width=1920, height=1080, stride=7680
  data: 1920 × 1080 × 4 = 8,294,400 bytes (BGRA)
  timestamp_us: 16_667

        │  NvencEncoder::encode()  [target: <5 ms]

EncodedPacket
  data: ~150,000 bytes (HEVC NAL units)
  is_keyframe: true
  timestamp_us: 16_667
  duration_us: 16_667   (1 frame @ 60 fps)

        │  FrameChunker::chunk()  (max_chunk_size = 1200)

NetworkChunk × 125   (125 × 1200 B = 150,000 B)
  packet_index: 0
  chunk_index:  0 … 124
  total_chunks: 125
  is_keyframe:  true
  timestamp_us: 16_667
```
