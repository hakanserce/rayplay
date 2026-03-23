use crate::{
    decoded_frame::DecodedFrame,
    encoder::{Codec, VideoError},
    packet::EncodedPacket,
    pipeline_mode::PipelineMode,
};

/// Returns the platform-appropriate hardware decoder.
///
/// On macOS, returns a [`VtDecoder`](crate::videotoolbox::VtDecoder) backed
/// by `VideoToolbox`.  On other platforms returns [`VideoError::UnsupportedPlatform`].
///
/// # Errors
///
/// Returns [`VideoError::UnsupportedPlatform`] on non-macOS, or
/// [`VideoError::DecodingFailed`] if the `VideoToolbox` session cannot be created.
pub fn create_decoder(
    codec: Codec,
    mode: PipelineMode,
) -> Result<Box<dyn VideoDecoder>, VideoError> {
    if mode == PipelineMode::Software {
        return create_software_decoder(codec);
    }

    #[cfg(target_os = "macos")]
    {
        use crate::videotoolbox::VtDecoder;
        VtDecoder::new(codec).map(|d| Box::new(d) as Box<dyn VideoDecoder>)
    }
    #[cfg(not(target_os = "macos"))]
    {
        create_software_decoder(codec)
    }
}

fn create_software_decoder(codec: Codec) -> Result<Box<dyn VideoDecoder>, VideoError> {
    #[cfg(feature = "ffmpeg-fallback")]
    {
        use crate::ffmpeg_dec::FfmpegDecoder;
        FfmpegDecoder::new(codec).map(|d| Box::new(d) as Box<dyn VideoDecoder>)
    }
    #[cfg(all(feature = "fallback", not(feature = "ffmpeg-fallback")))]
    {
        use crate::openh264_dec::OpenH264Decoder;
        OpenH264Decoder::new(codec).map(|d| Box::new(d) as Box<dyn VideoDecoder>)
    }
    #[cfg(not(any(feature = "fallback", feature = "ffmpeg-fallback")))]
    {
        let _ = codec;
        Err(VideoError::UnsupportedPlatform)
    }
}

/// Trait for hardware or software video decoders.
///
/// Implementations must be `Send` so they can be driven from a dedicated
/// decode thread. The `decode` → `flush` lifecycle mirrors the `VideoToolbox`
/// asynchronous session model.
pub trait VideoDecoder: Send {
    /// Submits a compressed packet for decoding.
    ///
    /// Returns `Ok(Some(frame))` when a decoded frame is immediately available,
    /// `Ok(None)` when the decoder is buffering, or an error.
    ///
    /// # Errors
    ///
    /// - `VideoError::CorruptPacket` — bitstream is undecodable (truncated NAL
    ///   units, invalid header, etc.).
    /// - `VideoError::DecodingFailed` — hardware or session error.
    fn decode(&mut self, packet: &EncodedPacket) -> Result<Option<DecodedFrame>, VideoError>;

    /// Flushes any buffered frames and returns all remaining decoded frames.
    ///
    /// Call this at end-of-stream or before reconfiguring the decoder.
    ///
    /// # Errors
    ///
    /// Returns `VideoError::DecodingFailed` if flushing the decode session fails.
    fn flush(&mut self) -> Result<Vec<DecodedFrame>, VideoError>;

    /// Returns the codec this decoder handles.
    fn codec(&self) -> Codec;
}

#[cfg(test)]
mod tests;
