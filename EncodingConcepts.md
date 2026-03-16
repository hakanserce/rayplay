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
  │  VideoFragmenter::fragment()
  ▼
Vec<VideoFragment>
  │
  │  QUIC unreliable datagrams (RFC 9221)   ── rayplay-network ──►
  ▼
Client
  │
  │  VideoReassembler::ingest()
  ▼
EncodedPacket
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
- **Consumed by:** `VideoFragmenter::fragment()` in `rayplay-network`

Typical sizes over HEVC at 1080p60:

```
Keyframe  ████████████████████████████████████  ~150 kB
P-frame   ████                                  ~20 kB
```

---

### `VideoFragmenter` (in `rayplay-network`)

Splits an `EncodedPacket` into QUIC-datagram-sized `VideoFragment`s.

- **Default payload size:** 1188 bytes (`MAX_FRAGMENT_PAYLOAD`) — fits within PMTU after 12-byte header and QUIC/UDP headers
- **Assigns a monotonically increasing `frame_id`** (wraps at `u32::MAX`) shared across all fragments of one packet
- **Produced by:** calling `VideoFragmenter::fragment(packet)`
- **Output:** `Vec<VideoFragment>`

```
EncodedPacket  (e.g. 3564 bytes)
│
├──► VideoFragment { frame_id: 7, frag_index: 0, frag_total: 3, payload: [1188 B] }
├──► VideoFragment { frame_id: 7, frag_index: 1, frag_total: 3, payload: [1188 B] }
└──► VideoFragment { frame_id: 7, frag_index: 2, frag_total: 3, payload: [1188 B] }
```

The `frame_id` wraps at `u32::MAX`. `frag_total` is capped at `u16::MAX`
(≈ 78 MB per packet at the default payload size — unreachable in practice).

---

### `VideoFragment` (in `rayplay-network`)

A single QUIC unreliable datagram payload with a 12-byte wire header.

- **Fields:** `frame_id`, `frag_index`, `frag_total`, `channel`, `flags`, `payload`
- **`flags`:** bit 0 = `FLAG_KEYFRAME`; set on all fragments of a keyframe
- **`channel`:** `Channel::Video` (value 0); audio/input channels added in future UCs
- **Produced by:** `VideoFragmenter::fragment()`
- **Consumed by:** `VideoReassembler::ingest()` after being received from the QUIC layer

Wire format (12-byte big-endian header + payload):

```
 0               1               2               3
 0 1 2 3 4 5 6 7 0 1 2 3 4 5 6 7 0 1 2 3 4 5 6 7 0 1 2 3 4 5 6 7
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                         frame_id (u32)                         |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|      frag_index (u16)         |       frag_total (u16)        |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|    channel (u8)   |  flags (u8)|        reserved (u16)        |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                       payload (variable)                       |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

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

        │  VideoFragmenter::fragment()  (max_payload = 1188)

VideoFragment × 127   (126 × 1188 B + 1 × 372 B = 150,000 B)
  frame_id:    0
  frag_index:  0 … 126
  frag_total:  127
  flags:       FLAG_KEYFRAME (bit 0)
  channel:     Video

        │  QUIC unreliable datagrams (RFC 9221)  ──LAN──►

VideoReassembler::ingest()   [buffers up to 4 in-flight frames]

        │  all 127 fragments received → complete

EncodedPacket
  data: ~150,000 bytes (HEVC NAL units, identical to sender)
  is_keyframe: true
```
