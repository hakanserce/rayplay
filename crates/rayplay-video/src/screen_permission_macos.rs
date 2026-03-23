//! macOS Screen Recording permission check via `CoreGraphics` FFI.
//!
//! Uses `CGPreflightScreenCaptureAccess` (macOS 10.15+) to check whether
//! the current process has Screen Recording permission, and
//! `CGRequestScreenCaptureAccess` to trigger the system prompt.
//!
//! Excluded from coverage: thin FFI wrapper over OS APIs.

unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

/// Returns `true` if Screen Recording permission is already granted.
#[must_use]
pub fn has_screen_recording_permission() -> bool {
    // SAFETY: `CGPreflightScreenCaptureAccess` is a stable CoreGraphics API
    // with no preconditions — safe to call from any thread.
    unsafe { CGPreflightScreenCaptureAccess() }
}

/// Requests Screen Recording permission from the user.
///
/// On first call, this opens System Settings to the Screen Recording pane.
/// Returns `true` if permission was already granted, `false` otherwise.
/// The user must grant permission and restart the app for it to take effect.
#[must_use]
pub fn request_screen_recording_permission() -> bool {
    // SAFETY: same as above — stable CoreGraphics API, no preconditions.
    unsafe { CGRequestScreenCaptureAccess() }
}
