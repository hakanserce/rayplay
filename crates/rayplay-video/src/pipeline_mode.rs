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
mod tests {
    use super::*;

    #[test]
    fn test_default_is_auto() {
        assert_eq!(PipelineMode::default(), PipelineMode::Auto);
    }

    #[test]
    fn test_debug_format() {
        assert_eq!(format!("{:?}", PipelineMode::Auto), "Auto");
        assert_eq!(format!("{:?}", PipelineMode::Software), "Software");
    }

    #[test]
    fn test_clone_and_copy() {
        let mode = PipelineMode::Software;
        let cloned = mode.clone();
        let copied = mode;

        assert_eq!(mode, cloned);
        assert_eq!(mode, copied);
    }

    #[test]
    fn test_partial_eq() {
        assert_eq!(PipelineMode::Auto, PipelineMode::Auto);
        assert_eq!(PipelineMode::Software, PipelineMode::Software);
        assert_ne!(PipelineMode::Auto, PipelineMode::Software);
    }
}
