/// DXGI Desktop Duplication screen capturer (Windows only).
///
/// Captures the primary monitor by acquiring frames from the
/// `IDXGIOutputDuplication` interface, copying each DirectX texture to a
/// CPU-readable staging texture, and returning the raw BGRA pixel data.
///
/// This is the foundation for the zero-copy pipeline described in ADR-001.
/// The current implementation uses one copy (GPU texture → staging texture →
/// host memory, i.e. Option A in ADR-001). Option B (NVENC direct from DXGI
/// texture) will be wired in during UC-002 once the NVENC encoder is ready.
#[cfg(target_os = "windows")]
mod inner {
    use std::time::{Duration, Instant};

    use tracing::{debug, instrument};
    use windows::{
        Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE,
        Win32::Graphics::Direct3D11::{
            D3D11_BIND_FLAG, D3D11_CPU_ACCESS_READ, D3D11_MAP_READ, D3D11_SDK_VERSION,
            D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING, D3D11CreateDevice, ID3D11Device,
            ID3D11DeviceContext, ID3D11Texture2D,
        },
        Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC},
        Win32::Graphics::Dxgi::{
            DXGI_ERROR_WAIT_TIMEOUT, IDXGIDevice, IDXGIOutput1, IDXGIOutputDuplication,
        },
        core::Interface as _,
    };

    use crate::capture::{CaptureConfig, CaptureError, CapturedFrame, ScreenCapturer};

    /// Screen capturer backed by DXGI Desktop Duplication.
    ///
    /// # Safety
    ///
    /// All D3D11 / DXGI calls must originate from the thread that owns this
    /// struct.  The capture loop in `rayplay-cli` runs on a single dedicated
    /// thread, satisfying this invariant.
    pub struct DxgiCapture {
        device: ID3D11Device,
        context: ID3D11DeviceContext,
        duplication: IDXGIOutputDuplication,
        width: u32,
        height: u32,
        timeout_ms: u32,
    }

    // SAFETY: see doc comment above — callers must not share across threads.
    unsafe impl Send for DxgiCapture {}

    impl DxgiCapture {
        /// Creates a new `DxgiCapture` for the primary display.
        ///
        /// # Errors
        ///
        /// Returns [`CaptureError::InitializationFailed`] if device creation or
        /// output duplication setup fails.
        #[instrument(skip(config))]
        pub fn new(config: CaptureConfig) -> Result<Self, CaptureError> {
            let (device, context) = create_d3d11_device()?;
            let (duplication, width, height) = create_duplication(&device)?;
            debug!(width, height, fps = config.target_fps, "DXGI capture ready");
            Ok(Self {
                device,
                context,
                duplication,
                width,
                height,
                timeout_ms: config.acquire_timeout_ms,
            })
        }
    }

    impl ScreenCapturer for DxgiCapture {
        #[instrument(skip(self))]
        fn capture_frame(&self) -> Result<CapturedFrame, CaptureError> {
            let timestamp = Instant::now();

            // 1. Acquire the next changed desktop frame.
            let desktop_texture = acquire_frame(&self.duplication, self.timeout_ms)?;

            // 2. Copy to a CPU-readable staging texture.
            let staging = copy_to_staging(
                &self.device,
                &self.context,
                &desktop_texture,
                self.width,
                self.height,
            )?;

            // 3. Map staging texture and read bytes.
            let (stride, data) = map_and_read(&self.context, &staging, self.height)?;

            // 4. Release the duplication frame now that we have the data.
            unsafe {
                let _ = self.duplication.ReleaseFrame();
            }

            Ok(CapturedFrame {
                width: self.width,
                height: self.height,
                stride,
                data,
                timestamp,
            })
        }

        fn resolution(&self) -> (u32, u32) {
            (self.width, self.height)
        }
    }

    // ── helpers ───────────────────────────────────────────────────────────────

    fn create_d3d11_device() -> Result<(ID3D11Device, ID3D11DeviceContext), CaptureError> {
        let mut device = None;
        let mut context = None;
        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                None,
                Default::default(),
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .map_err(|e| CaptureError::InitializationFailed(e.to_string()))?;
        }
        let device = device
            .ok_or_else(|| CaptureError::InitializationFailed("D3D11 device was null".into()))?;
        let context = context
            .ok_or_else(|| CaptureError::InitializationFailed("D3D11 context was null".into()))?;
        Ok((device, context))
    }

    fn create_duplication(
        device: &ID3D11Device,
    ) -> Result<(IDXGIOutputDuplication, u32, u32), CaptureError> {
        unsafe {
            let dxgi_device: IDXGIDevice = device
                .cast()
                .map_err(|e| CaptureError::InitializationFailed(e.to_string()))?;
            let adapter = dxgi_device
                .GetAdapter()
                .map_err(|e| CaptureError::InitializationFailed(e.to_string()))?;
            let output = adapter
                .EnumOutputs(0)
                .map_err(|e| CaptureError::InitializationFailed(e.to_string()))?;
            let output1: IDXGIOutput1 = output
                .cast()
                .map_err(|e| CaptureError::InitializationFailed(e.to_string()))?;

            let desc = output1
                .GetDesc()
                .map_err(|e| CaptureError::InitializationFailed(e.to_string()))?;
            let width = (desc.DesktopCoordinates.right - desc.DesktopCoordinates.left) as u32;
            let height = (desc.DesktopCoordinates.bottom - desc.DesktopCoordinates.top) as u32;

            let duplication = output1
                .DuplicateOutput(device)
                .map_err(|e| CaptureError::InitializationFailed(e.to_string()))?;

            Ok((duplication, width, height))
        }
    }

    fn acquire_frame(
        duplication: &IDXGIOutputDuplication,
        timeout_ms: u32,
    ) -> Result<ID3D11Texture2D, CaptureError> {
        let mut frame_info = Default::default();
        let mut resource = None;
        unsafe {
            duplication
                .AcquireNextFrame(timeout_ms, &mut frame_info, &mut resource)
                .map_err(|e| {
                    if e.code() == DXGI_ERROR_WAIT_TIMEOUT {
                        CaptureError::Timeout(Duration::from_millis(u64::from(timeout_ms)))
                    } else {
                        CaptureError::AcquireFailed(e.to_string())
                    }
                })?;
            let texture: ID3D11Texture2D = resource
                .ok_or_else(|| CaptureError::AcquireFailed("resource was null".into()))?
                .cast()
                .map_err(|e| CaptureError::AcquireFailed(e.to_string()))?;
            Ok(texture)
        }
    }

    fn copy_to_staging(
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
        src: &ID3D11Texture2D,
        width: u32,
        height: u32,
    ) -> Result<ID3D11Texture2D, CaptureError> {
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
            Usage: D3D11_USAGE_STAGING,
            BindFlags: D3D11_BIND_FLAG(0),
            CPUAccessFlags: D3D11_CPU_ACCESS_READ,
            MiscFlags: Default::default(),
        };
        let mut staging = None;
        unsafe {
            device
                .CreateTexture2D(&desc, None, Some(&mut staging))
                .map_err(|e| CaptureError::AcquireFailed(e.to_string()))?;
            let staging = staging
                .ok_or_else(|| CaptureError::AcquireFailed("staging texture was null".into()))?;
            context.CopyResource(&staging, src);
            Ok(staging)
        }
    }

    fn map_and_read(
        context: &ID3D11DeviceContext,
        texture: &ID3D11Texture2D,
        height: u32,
    ) -> Result<(u32, Vec<u8>), CaptureError> {
        unsafe {
            let mut mapped = Default::default();
            context
                .Map(texture, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .map_err(|e| CaptureError::AcquireFailed(e.to_string()))?;

            let stride = mapped.RowPitch;
            let byte_len = stride as usize * height as usize;
            let data = std::slice::from_raw_parts(mapped.pData.cast::<u8>(), byte_len).to_vec();

            context.Unmap(texture, 0);
            Ok((stride, data))
        }
    }
}

#[cfg(target_os = "windows")]
pub use inner::DxgiCapture;
