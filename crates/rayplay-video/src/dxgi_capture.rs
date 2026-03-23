/// DXGI Desktop Duplication screen capturer (Windows only).
///
/// Captures the primary monitor by acquiring frames from the
/// `IDXGIOutputDuplication` interface.  Supports two capture modes:
///
/// - **CPU readback** ([`ScreenCapturer`]): copies to a staging texture then
///   maps into host memory (ADR-001 Option A).
/// - **Zero-copy** ([`ZeroCopyCapturer`]): returns the GPU texture pointer
///   directly for NVENC to consume (ADR-001 Option B).
#[cfg(target_os = "windows")]
mod inner {
    use std::cell::Cell;
    use std::ffi::c_void;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use tracing::{debug, instrument};
    use windows::Win32::Graphics::Direct3D11::{
        D3D11_BIND_FLAG, D3D11_CPU_ACCESS_READ, D3D11_MAP_READ, D3D11_TEXTURE2D_DESC,
        D3D11_USAGE_STAGING, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
    };
    use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
    use windows::Win32::Graphics::Dxgi::{
        DXGI_ERROR_WAIT_TIMEOUT, IDXGIDevice, IDXGIOutput1, IDXGIOutputDuplication,
    };
    use windows::core::Interface as _;

    use crate::capture::{
        CaptureConfig, CaptureError, CapturedFrame, CapturedTexture, ScreenCapturer,
        ZeroCopyCapturer,
    };
    use crate::d3d11_device::SharedD3D11Device;

    /// Screen capturer backed by DXGI Desktop Duplication.
    ///
    /// Shares a [`SharedD3D11Device`] with the NVENC encoder so that
    /// textures acquired here can be registered directly for encoding.
    ///
    /// # Safety
    ///
    /// All D3D11 / DXGI calls must originate from the thread that owns this
    /// struct.  The capture loop in `rayplay-cli` runs on a single dedicated
    /// thread, satisfying this invariant.
    ///
    /// `device` is wrapped in `Arc` for shared ownership with the encoder.
    /// Despite `Arc` normally implying `Sync`, both `DxgiCapture` and
    /// `NvencEncoder` access the device only from this same single thread —
    /// `SharedD3D11Device` is `Send` but intentionally not `Sync`.
    pub struct DxgiCapture {
        device: Arc<SharedD3D11Device>,
        duplication: IDXGIOutputDuplication,
        width: u32,
        height: u32,
        timeout_ms: u32,
        /// Raw COM pointer to the most recently acquired DXGI texture.
        /// Non-null between `acquire_texture()` and `release_frame()`.
        /// Uses `Cell` so both methods can take `&self` (single-thread invariant).
        pending_texture: Cell<*mut c_void>,
    }

    // SAFETY: see doc comment above — callers must not share across threads.
    unsafe impl Send for DxgiCapture {}

    impl DxgiCapture {
        /// Creates a new `DxgiCapture` for the primary display using a shared device.
        ///
        /// # Errors
        ///
        /// Returns [`CaptureError::InitializationFailed`] if output duplication
        /// setup fails.
        #[instrument(skip_all)]
        pub fn new(
            config: CaptureConfig,
            device: Arc<SharedD3D11Device>,
        ) -> Result<Self, CaptureError> {
            let (duplication, width, height) = create_duplication(device.device())?;
            debug!(width, height, fps = config.target_fps, "DXGI capture ready");
            Ok(Self {
                device,
                duplication,
                width,
                height,
                timeout_ms: config.acquire_timeout_ms,
                pending_texture: Cell::new(std::ptr::null_mut()),
            })
        }
    }

    impl ScreenCapturer for DxgiCapture {
        #[instrument(skip(self))]
        fn capture_frame(&mut self) -> Result<CapturedFrame, CaptureError> {
            let timestamp = Instant::now();

            let desktop_texture = acquire_frame(&self.duplication, self.timeout_ms)?;

            let staging = copy_to_staging(
                self.device.device(),
                self.device.context(),
                &desktop_texture,
                self.width,
                self.height,
            )?;

            let (stride, data) = map_and_read(self.device.context(), &staging, self.height)?;

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

    impl ZeroCopyCapturer for DxgiCapture {
        /// Acquires the next frame and returns its GPU texture pointer.
        ///
        /// Transfers COM ownership to `pending_texture` via `into_raw()`,
        /// keeping the refcount alive until `release_frame()` reconstructs
        /// and drops the object.
        fn acquire_texture(&self) -> Result<CapturedTexture, CaptureError> {
            let desktop_texture = acquire_frame(&self.duplication, self.timeout_ms)?;
            // Transfer COM ownership to a raw pointer — refcount stays alive.
            let raw = desktop_texture.into_raw();
            self.pending_texture.set(raw);
            Ok(CapturedTexture {
                texture_ptr: raw,
                width: self.width,
                height: self.height,
            })
        }

        /// Releases the pending DXGI frame.
        ///
        /// Reconstructs the `ID3D11Texture2D` from the stored raw pointer
        /// (decrementing the COM refcount on drop), then calls `ReleaseFrame`
        /// to return the frame to the duplication API.
        fn release_frame(&self) {
            let raw = self.pending_texture.replace(std::ptr::null_mut());
            if !raw.is_null() {
                unsafe {
                    // Reconstruct + drop — this calls Release() on the COM object.
                    drop(ID3D11Texture2D::from_raw(raw));
                    let _ = self.duplication.ReleaseFrame();
                }
            }
        }

        fn resolution(&self) -> (u32, u32) {
            (self.width, self.height)
        }
    }

    // ── helpers ───────────────────────────────────────────────────────────────

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
            BindFlags: D3D11_BIND_FLAG(0).0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0,
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
