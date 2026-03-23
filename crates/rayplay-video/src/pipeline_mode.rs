//! Pipeline mode selection for video capture, encoding, and decoding components.

/// Controls whether the video pipeline uses hardware or software components.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PipelineMode {
    /// Try hardware first, fall back to software.
    #[default]
    Auto,
    /// Force software path — skip hardware even on supported platforms.
    Software,
}

#[cfg(test)]
mod tests;
