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
    let cloned = mode;
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
