//! End-to-end integration tests for the Windows zero-copy DXGI+NVENC pipeline.
//!
//! These tests require a real Windows machine with an NVIDIA GPU and are gated
//! behind the `hw-codec-tests` feature flag.
//!
//! Run with: `cargo test --features hw-codec-tests -p rayplay-cli --test e2e_win_nvenc`

#![cfg(all(target_os = "windows", feature = "hw-codec-tests"))]

#[path = "e2e_helpers.rs"]
mod e2e_helpers;

use std::ffi::c_void;
use std::time::{Duration, Instant};

use e2e_helpers::{collect_frames, run_client_recv_decode, setup_transport};
use rayplay_video::{
    Codec, DecodedFrame, EncoderConfig, OpenH264Decoder,
    d3d11_device::SharedD3D11Device,
    encoder::{EncoderInput, GpuTextureHandle, VideoEncoder},
    nvenc::NvencEncoder,
    packet::EncodedPacket,
};
use tokio_util::sync::CancellationToken;

/// Creates a D3D11 texture filled with a solid color test pattern.
unsafe fn create_test_texture(device: &SharedD3D11Device, width: u32, height: u32) -> *mut c_void {
    use windows::Win32::Graphics::Direct3D11::*;
    use windows::Win32::Graphics::Dxgi::Common::*;

    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_SHADER_RESOURCE,
        CPUAccessFlags: D3D11_CPU_ACCESS_FLAG(0),
        MiscFlags: D3D11_RESOURCE_MISC_FLAG(0),
    };

    let pixel_count = (width * height) as usize;
    let mut pixels = vec![0u8; pixel_count * 4];
    for i in 0..pixel_count {
        pixels[i * 4] = 255; // B
        pixels[i * 4 + 1] = 0; // G
        pixels[i * 4 + 2] = 0; // R
        pixels[i * 4 + 3] = 255; // A
    }

    let init_data = D3D11_SUBRESOURCE_DATA {
        pSysMem: pixels.as_ptr() as *const c_void,
        SysMemPitch: width * 4,
        SysMemSlicePitch: 0,
    };

    let mut texture = None;
    unsafe {
        device
            .device()
            .CreateTexture2D(&desc, Some(&init_data), Some(&mut texture))
            .expect("Failed to create test texture");
    }

    let texture = texture.expect("Texture was not created");
    unsafe { windows::core::Interface::as_raw(&texture) as *mut c_void }
}

/// Drives a zero-copy encode loop using NVENC with synthetic D3D11 textures,
/// sending encoded packets over the QUIC transport.
#[allow(clippy::cast_possible_truncation)]
async fn run_zero_copy_host(
    mut transport: rayplay_network::QuicVideoTransport,
    frame_count: u32,
    width: u32,
    height: u32,
    codec: Codec,
    token: CancellationToken,
) -> anyhow::Result<()> {
    let (packet_tx, mut packet_rx) = tokio::sync::mpsc::channel::<anyhow::Result<EncodedPacket>>(4);
    let encode_token = token.clone();

    let encode_handle = tokio::task::spawn_blocking(move || {
        let device = SharedD3D11Device::new().expect("D3D11 device");
        let config = EncoderConfig::with_codec(width, height, 60, codec);
        let mut encoder =
            NvencEncoder::new(config, device.device_ptr()).expect("NVENC encoder init");

        let texture_ptr = unsafe { create_test_texture(&device, width, height) };
        let session_start = Instant::now();

        for i in 0..frame_count {
            if encode_token.is_cancelled() {
                break;
            }

            let timestamp_us = session_start.elapsed().as_micros() as u64;
            let input = EncoderInput::GpuTexture {
                handle: GpuTextureHandle(texture_ptr),
                width,
                height,
                timestamp_us,
            };

            match encoder.encode(input) {
                Ok(Some(packet)) => {
                    if packet_tx.blocking_send(Ok(packet)).is_err() {
                        break;
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    let _ = packet_tx
                        .blocking_send(Err(anyhow::anyhow!("encode frame {i} failed: {e}")));
                    return;
                }
            }
        }

        // Flush remaining
        if let Ok(flushed) = encoder.flush() {
            for p in flushed {
                if packet_tx.blocking_send(Ok(p)).is_err() {
                    break;
                }
            }
        }
    });

    loop {
        tokio::select! {
            () = token.cancelled() => break,
            packet = packet_rx.recv() => {
                match packet {
                    Some(Ok(p)) => transport.send_video(&p).await?,
                    Some(Err(e)) => return Err(e),
                    None => break,
                }
            }
        }
    }

    encode_handle
        .await
        .map_err(|e| anyhow::anyhow!("encode thread panicked: {e}"))?;
    Ok(())
}

/// Full zero-copy pipeline: NVENC H264 encode → QUIC → OpenH264 decode.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_zero_copy_h264_roundtrip() {
    let (server, client) = setup_transport().await.expect("transport setup");

    let token = CancellationToken::new();
    let (frame_tx, frame_rx) = crossbeam_channel::bounded::<DecodedFrame>(16);

    let host_token = token.clone();
    let host = tokio::spawn(async move {
        run_zero_copy_host(server, 5, 64, 64, Codec::H264, host_token).await
    });

    let client_token = token.clone();
    let decoder = Box::new(OpenH264Decoder::new(Codec::H264).expect("decoder"));
    let client_handle = tokio::spawn(async move {
        run_client_recv_decode(client, decoder, frame_tx, client_token).await
    });

    let frames = collect_frames(&frame_rx, 3, Duration::from_secs(15));
    token.cancel();

    let _ = host.await;
    let _ = client_handle.await;

    assert!(
        !frames.is_empty(),
        "expected at least 1 decoded frame from NVENC H264 zero-copy pipeline"
    );
    for frame in &frames {
        assert_eq!(frame.width, 64);
        assert_eq!(frame.height, 64);
        assert!(
            !frame.data.is_empty(),
            "decoded frame data should not be empty"
        );
    }
}

/// Full zero-copy pipeline with HEVC: NVENC HEVC encode → QUIC transport.
///
/// Note: We only verify the encoded packets are produced (no HEVC decoder in
/// the software stack yet), so this test validates the NVENC encode path alone.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_zero_copy_hevc_encode_produces_packets() {
    let device = SharedD3D11Device::new().expect("D3D11 device");
    let config = EncoderConfig::with_codec(64, 64, 60, Codec::Hevc);
    let mut encoder =
        NvencEncoder::new(config, device.device_ptr()).expect("NVENC HEVC encoder init");

    let texture_ptr = unsafe { create_test_texture(&device, 64, 64) };

    let mut packets = Vec::new();
    for i in 0..5u64 {
        let input = EncoderInput::GpuTexture {
            handle: GpuTextureHandle(texture_ptr),
            width: 64,
            height: 64,
            timestamp_us: i * 16_667,
        };
        if let Some(packet) = encoder.encode(input).expect("encode") {
            packets.push(packet);
        }
    }

    let flushed = encoder.flush().expect("flush");
    packets.extend(flushed);

    assert!(
        packets.len() >= 3,
        "expected at least 3 HEVC packets, got {}",
        packets.len()
    );
    for packet in &packets {
        assert!(
            !packet.data.is_empty(),
            "HEVC packet data should not be empty"
        );
    }
}
