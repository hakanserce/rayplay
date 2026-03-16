# RayPlay Networking

A guide to the network transport layer in `rayplay-network`.

## Overview

RayPlay uses **QUIC** (via `quinn` ≥0.11) as its sole transport protocol.
QUIC runs over UDP and provides TLS 1.3 encryption, multiplexed streams, and
RFC 9221 unreliable datagrams in a single connection — exactly the mix of
reliability primitives that game streaming requires (ADR-003).

```
Host (Windows)                              Client (macOS / Android)
─────────────────────────────────────────────────────────────────────
EncodedPacket
  │
  │  VideoFragmenter::fragment()
  ▼
Vec<VideoFragment>
  │
  │  QuicVideoTransport::send_video()
  │  connection.send_datagram()             connection.read_datagram()
  │  ── QUIC unreliable datagrams ──────────────────────────────────►
                                            VideoFragment::decode()
                                              │
                                              │  VideoReassembler::ingest()
                                              ▼
                                            EncodedPacket
```

---

## Channel Mapping

One QUIC connection carries every traffic type. Each type maps to the QUIC
primitive that best matches its reliability requirement:

| Channel | QUIC primitive | Rationale |
|---------|----------------|-----------|
| Video fragments | Unreliable datagrams (RFC 9221) | Loss-tolerant; avoids HOL blocking |
| Audio frames | Unreliable datagrams (RFC 9221) | Loss-tolerant; fits in one datagram |
| Mouse position / scroll | Unreliable datagrams (RFC 9221) | Latest value supersedes older ones |
| Keyboard + mouse buttons | Reliable unidirectional stream | Ordered; no loss tolerated |
| Control / session | Reliable bidirectional stream | Handshake, keepalive, reconnect |

> **UC-003 scope:** only video datagrams are implemented. Audio, input, and
> control channels are added in subsequent UCs.

---

## Crate Structure

```
crates/rayplay-network/
├── src/
│   ├── lib.rs           # Public re-exports
│   ├── wire.rs          # VideoFragment wire format, encode/decode, TransportError
│   ├── fragmenter.rs    # VideoFragmenter — splits EncodedPacket → Vec<VideoFragment>
│   ├── reassembler.rs   # VideoReassembler — reassembles VideoFragment → EncodedPacket
│   └── transport.rs     # QuicVideoTransport, QuicListener — QUIC endpoint management
└── benches/
    └── transport.rs     # Criterion benchmarks
```

The `NetworkTransport` trait lives in `rayplay-core` so that `rayplay-video`
and `rayplay-input` can depend on the abstraction without pulling in `quinn`.

---

## Wire Format

Every QUIC datagram starts with a fixed 12-byte big-endian header:

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

| Field | Type | Description |
|-------|------|-------------|
| `frame_id` | `u32` BE | Monotonically increasing frame counter (wraps at `u32::MAX`) |
| `frag_index` | `u16` BE | Zero-based index of this fragment within the frame |
| `frag_total` | `u16` BE | Total number of fragments for this frame (≥1) |
| `channel` | `u8` | `0` = Video; future values for audio, input |
| `flags` | `u8` | Bit 0 = `FLAG_KEYFRAME`; all other bits reserved |
| `reserved` | `u16` | Must be zero on send; ignored on receive |
| `payload` | bytes | Raw encoded data (up to `MAX_FRAGMENT_PAYLOAD` = 1188 bytes) |

Constants in `wire.rs`:

| Constant | Value | Meaning |
|----------|-------|---------|
| `HEADER_LEN` | 12 | Bytes occupied by the fixed header |
| `MAX_FRAGMENT_PAYLOAD` | 1188 | Max payload bytes per fragment (PMTU 1200 − 12) |
| `FLAG_KEYFRAME` | `0x01` | Set on all fragments of an IDR frame |

---

## Fragmentation

`VideoFragmenter` (in `fragmenter.rs`) splits one `EncodedPacket` into a
`Vec<VideoFragment>` ready for transmission:

- Each fragment carries at most `MAX_FRAGMENT_PAYLOAD` bytes.
- All fragments of one packet share the same `frame_id`.
- `frame_id` is incremented (wrapping) after each non-empty packet.
- Empty packets return an empty `Vec` without incrementing the counter.
- `FLAG_KEYFRAME` is copied to every fragment when `packet.is_keyframe` is true.

```
EncodedPacket  (~150 kB keyframe)
│
├──► VideoFragment { frame_id: 42, frag_index: 0,   frag_total: 127, payload: [1188 B] }
├──► VideoFragment { frame_id: 42, frag_index: 1,   frag_total: 127, payload: [1188 B] }
│    …
└──► VideoFragment { frame_id: 42, frag_index: 126, frag_total: 127, payload: [372 B]  }
```

---

## Reassembly

`VideoReassembler` (in `reassembler.rs`) ingests fragments and returns a
complete `EncodedPacket` once all fragments for a frame are received:

- Maintains a `HashMap<frame_id, PendingFrame>`.
- Bounded to **4 in-flight frames** (`MAX_IN_FLIGHT_FRAMES`) — when a new
  `frame_id` arrives and the buffer is full, the oldest incomplete frame is
  evicted (dropped).
- Duplicate fragments are silently ignored.
- Fragments with `frag_index ≥ frag_total` are silently ignored.
- `evict_before(n)` discards all incomplete frames with `frame_id < n`.

The sliding-window eviction in `recv_video` calls `evict_before` when the
receiver is more than 4 frames behind the sender:

```rust
if frag.frame_id >= window {
    reassembler.evict_before(frag.frame_id - window);
}
```

This bounds reassembler memory without blocking on lost packets.

---

## Transport API

### Server (host) side

```rust
// 1. Bind and get the certificate immediately (non-blocking).
let (listener, cert_der) = QuicVideoTransport::listen("0.0.0.0:5000".parse()?)?;

// 2. Distribute cert_der to the client out-of-band
//    (PIN pairing, QR code — future ADR-007 SPAKE2 flow).

// 3. Block until the client connects.
let mut host = listener.accept().await?;

// 4. Send encoded frames.
host.send_video(&encoded_packet).await?;
```

### Client side

```rust
// server_cert is the DER bytes received out-of-band from the host.
let mut client = QuicVideoTransport::connect(server_addr, server_cert).await?;

// Receive reassembled frames.
let packet: EncodedPacket = client.recv_video().await?;
```

### Why two-phase `listen` / `accept`?

The server must share its self-signed TLS certificate with the client before
the client can connect. `listen()` returns the certificate immediately without
blocking, so the application can distribute it while waiting for the client.
`QuicListener::accept()` then blocks until the QUIC handshake completes.

---

## TLS

A self-signed certificate is generated per session via `rcgen`. The client
pins exactly that certificate — no CA chain is needed on a LAN. ADR-007 will
replace this with a SPAKE2 PIN-pairing flow for secure key exchange without
manual certificate distribution.

---

## Error Handling

All transport errors are represented by `TransportError` (in `wire.rs`):

| Variant | Cause |
|---------|-------|
| `DatagramTooShort` | Received datagram shorter than `HEADER_LEN` |
| `InvalidFragTotal` | `frag_total` is zero |
| `FragIndexOutOfRange` | `frag_index ≥ frag_total` |
| `UnknownChannel` | `channel` byte is not a known `Channel` value |
| `TlsError(String)` | TLS certificate generation or parsing failed |
| `EndpointClosed` | QUIC endpoint shut down before a connection arrived |
| `Connection` | quinn `ConnectionError` (remote closed, timeout, etc.) |
| `Connect` | quinn `ConnectError` (invalid address, TLS handshake failed) |
| `SendDatagram` | quinn `SendDatagramError` (datagram too large, connection closed) |
| `Io` | OS-level I/O error (bind failed, etc.) |

---

## Performance

Target: **<1 ms transport overhead** per video packet on a LAN (ADR-003).

QUIC unreliable datagrams bypass the congestion window (RFC 9221 §5), so
video throughput is limited only by UDP socket bandwidth. On a 1 Gbps LAN a
150 kB keyframe (127 fragments × 1200-byte datagrams) fits in a single burst
of < 1.5 ms at wire speed.

Criterion benchmarks are in `benches/transport.rs`:

```
VideoFragmenter/fragment/1000       — ~1 µs
VideoFragmenter/fragment/100000     — ~85 µs
VideoReassembler/ingest/single_fragment_frame  — ~200 ns
VideoReassembler/ingest/ten_fragment_frame     — ~1.8 µs
Wire/roundtrip/full_payload         — ~300 ns
```

Run with:

```bash
cargo make benchmark
```
