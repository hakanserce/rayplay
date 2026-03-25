//! Hardware end-to-end tests for NVENC encoding.
//!
//! These tests require a real Windows machine with an NVIDIA GPU and are gated
//! behind the `hw-codec-tests` feature flag.
//!
//! Run with: `cargo test --features hw-codec-tests -p rayplay-video --test nvenc_hw_e2e`

#![cfg(all(target_os = "windows", feature = "hw-codec-tests"))]

use std::ffi::c_void;

use rayplay_video::encoder::{Codec, EncoderConfig, EncoderInput, GpuTextureHandle, VideoEncoder};

// Re-use the D3D11 device creation from the d3d11_device module.
use rayplay_video::d3d11_device::SharedD3D11Device;

/// Creates a D3D11 texture filled with a solid color test pattern.
///
/// Returns `(texture_ptr, width, height)`.
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

    // Create a solid blue test pattern (BGRA format)
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
    // Get the raw COM pointer
    unsafe { windows::core::Interface::as_raw(&texture) as *mut c_void }
}

/// Test: Create an NVENC encoder, encode a single frame, verify output is valid.
#[test]
fn test_nvenc_encode_single_frame_h264() {
    let device = SharedD3D11Device::new().expect("Failed to create D3D11 device");

    let config = EncoderConfig {
        width: 1920,
        height: 1080,
        fps: 60,
        codec: Codec::H264,
        ..Default::default()
    };

    let mut encoder = rayplay_video::nvenc::NvencEncoder::new(config, device.device_ptr())
        .expect("Failed to create NVENC encoder");

    // Create a test texture
    let texture_ptr = unsafe { create_test_texture(&device, 1920, 1080) };

    // Encode one frame
    let input = EncoderInput::GpuTexture {
        handle: GpuTextureHandle(texture_ptr),
        width: 1920,
        height: 1080,
        timestamp_us: 0,
    };
    let packet = encoder.encode(input).expect("Encoding failed");

    // Should produce output
    assert!(
        packet.is_some(),
        "First frame should produce encoded output"
    );
    let packet = packet.unwrap();
    assert!(!packet.data.is_empty(), "Encoded data should not be empty");

    // H264 NAL unit header check: first bytes should be 0x00 0x00 0x00 0x01 (start code)
    // or 0x00 0x00 0x01
    let has_start_code = (packet.data.len() >= 4
        && packet.data[0] == 0x00
        && packet.data[1] == 0x00
        && packet.data[2] == 0x00
        && packet.data[3] == 0x01)
        || (packet.data.len() >= 3
            && packet.data[0] == 0x00
            && packet.data[1] == 0x00
            && packet.data[2] == 0x01);
    assert!(
        has_start_code,
        "Encoded H264 data should start with NAL start code, got: {:02x?}",
        &packet.data[..packet.data.len().min(8)]
    );

    println!(
        "Successfully encoded 1 H264 frame: {} bytes",
        packet.data.len()
    );
}

/// Test: Create an NVENC encoder, encode a single HEVC frame, verify output.
#[test]
fn test_nvenc_encode_single_frame_hevc() {
    let device = SharedD3D11Device::new().expect("Failed to create D3D11 device");

    let config = EncoderConfig {
        width: 1920,
        height: 1080,
        fps: 60,
        codec: Codec::Hevc,
        ..Default::default()
    };

    let mut encoder = rayplay_video::nvenc::NvencEncoder::new(config, device.device_ptr())
        .expect("Failed to create NVENC encoder");

    let texture_ptr = unsafe { create_test_texture(&device, 1920, 1080) };

    let input = EncoderInput::GpuTexture {
        handle: GpuTextureHandle(texture_ptr),
        width: 1920,
        height: 1080,
        timestamp_us: 0,
    };
    let packet = encoder.encode(input).expect("Encoding failed");

    assert!(
        packet.is_some(),
        "First frame should produce encoded output"
    );
    let packet = packet.unwrap();
    assert!(!packet.data.is_empty(), "Encoded data should not be empty");

    println!(
        "Successfully encoded 1 HEVC frame: {} bytes",
        packet.data.len()
    );
}

/// Test: Encode multiple frames and verify consistent output.
#[test]
fn test_nvenc_encode_multiple_frames_h264() {
    let device = SharedD3D11Device::new().expect("Failed to create D3D11 device");

    let config = EncoderConfig {
        width: 1280,
        height: 720,
        fps: 60,
        codec: Codec::H264,
        ..Default::default()
    };

    let mut encoder = rayplay_video::nvenc::NvencEncoder::new(config, device.device_ptr())
        .expect("Failed to create NVENC encoder");

    let texture_ptr = unsafe { create_test_texture(&device, 1280, 720) };

    let mut total_bytes = 0;
    let mut frame_count = 0;

    for i in 0..10 {
        let input = EncoderInput::GpuTexture {
            handle: GpuTextureHandle(texture_ptr),
            width: 1280,
            height: 720,
            timestamp_us: i * 16_667, // ~60fps
        };
        let packet = encoder
            .encode(input)
            .unwrap_or_else(|e| panic!("Encoding frame {i} failed: {e}"));

        if let Some(p) = packet {
            assert!(!p.data.is_empty(), "Frame {i} produced empty data");
            total_bytes += p.data.len();
            frame_count += 1;
        }
    }

    // Flush remaining frames
    let flushed = encoder.flush().expect("Flush failed");
    for p in &flushed {
        total_bytes += p.data.len();
        frame_count += 1;
    }

    assert!(
        frame_count >= 10,
        "Should have encoded at least 10 frames, got {frame_count}"
    );
    println!("Encoded {frame_count} H264 frames, {total_bytes} total bytes");
}
