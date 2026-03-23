/// Shared D3D11 device for capture and encoding (Windows only).
///
/// Wraps an `ID3D11Device` and its immediate context so that both the
/// DXGI capturer and the NVENC encoder can share the same GPU device.
/// This is required for zero-copy encoding: NVENC must register textures
/// that belong to the same D3D11 device used by Desktop Duplication.
#[cfg(target_os = "windows")]
#[allow(
    clippy::default_trait_access,
    clippy::borrow_as_ptr,
    clippy::must_use_candidate
)]
mod inner {
    use std::ffi::c_void;

    use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
    use windows::Win32::Graphics::Direct3D11::{
        D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext,
    };

    use crate::capture::CaptureError;

    /// A D3D11 device + context pair shared between capture and encode.
    ///
    /// # Safety
    ///
    /// All D3D11 API calls through this device must originate from a single
    /// thread.  The capture-and-encode pipeline in `rayplay-cli` runs on one
    /// dedicated blocking thread, satisfying this invariant.
    pub struct SharedD3D11Device {
        device: ID3D11Device,
        context: ID3D11DeviceContext,
    }

    // SAFETY: see doc comment — callers ensure single-thread access.
    unsafe impl Send for SharedD3D11Device {}

    impl SharedD3D11Device {
        /// Creates a new D3D11 hardware device.
        ///
        /// # Errors
        ///
        /// Returns [`CaptureError::InitializationFailed`] if device creation fails.
        pub fn new() -> Result<Self, CaptureError> {
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
            let device = device.ok_or_else(|| {
                CaptureError::InitializationFailed("D3D11 device was null".into())
            })?;
            let context = context.ok_or_else(|| {
                CaptureError::InitializationFailed("D3D11 context was null".into())
            })?;
            Ok(Self { device, context })
        }

        /// Returns a reference to the underlying `ID3D11Device`.
        pub fn device(&self) -> &ID3D11Device {
            &self.device
        }

        /// Returns a reference to the immediate device context.
        pub fn context(&self) -> &ID3D11DeviceContext {
            &self.context
        }

        /// Returns an opaque pointer to the `ID3D11Device` for NVENC session creation.
        ///
        /// # Safety
        ///
        /// The returned pointer is valid for the lifetime of this `SharedD3D11Device`.
        /// The caller must not dereference it after this struct is dropped.
        pub fn device_ptr(&self) -> *mut c_void {
            let raw: *const ID3D11Device = &self.device;
            raw.cast_mut().cast::<c_void>()
        }
    }
}

#[cfg(target_os = "windows")]
pub use inner::SharedD3D11Device;
