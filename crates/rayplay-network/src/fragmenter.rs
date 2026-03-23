//! Splits [`EncodedPacket`]s into [`VideoFragment`]s for QUIC datagram transport.

use rayplay_core::packet::EncodedPacket;

use crate::wire::{Channel, FLAG_KEYFRAME, MAX_FRAGMENT_PAYLOAD, VideoFragment};

/// Splits outgoing [`EncodedPacket`]s into fixed-size [`VideoFragment`]s
/// that fit within a single QUIC unreliable datagram.
///
/// Each call to [`VideoFragmenter::fragment`] assigns a monotonically
/// increasing `frame_id` (wrapping at `u32::MAX`) and produces one fragment
/// per `max_payload`-byte slice of the packet data.
pub struct VideoFragmenter {
    frame_counter: u32,
    max_payload: usize,
}

impl VideoFragmenter {
    /// Creates a new fragmenter with a custom maximum payload size per fragment.
    ///
    /// # Panics
    ///
    /// Panics if `max_payload` is zero.
    #[must_use]
    pub fn new(max_payload: usize) -> Self {
        assert!(max_payload > 0, "max_payload must be > 0");
        Self {
            frame_counter: 0,
            max_payload,
        }
    }

    /// Creates a fragmenter using [`MAX_FRAGMENT_PAYLOAD`] (1188 bytes).
    #[must_use]
    pub fn with_default_payload() -> Self {
        Self::new(MAX_FRAGMENT_PAYLOAD)
    }

    /// Splits `packet` into a `Vec<VideoFragment>` ready for transmission.
    ///
    /// - Returns an empty `Vec` if `packet.data` is empty.
    /// - Each fragment carries at most `max_payload` bytes.
    /// - The `frame_id` is the same for all fragments of a single packet and
    ///   is incremented (wrapping) after a non-empty packet is processed.
    /// - The `FLAG_KEYFRAME` bit is set on all fragments when `packet.is_keyframe`.
    ///
    /// # Panics
    ///
    /// Panics if the encoded packet is so large that the number of fragments would
    /// exceed `u16::MAX`.
    #[must_use]
    pub fn fragment(&mut self, packet: &EncodedPacket) -> Vec<VideoFragment> {
        if packet.data.is_empty() {
            return Vec::new();
        }

        let chunks: Vec<&[u8]> = packet.data.chunks(self.max_payload).collect();
        let frag_total_usize = chunks.len();
        assert!(
            u16::try_from(frag_total_usize).is_ok(),
            "too many fragments: encoded packet too large for u16 frag_total"
        );
        #[allow(clippy::cast_possible_truncation)]
        let frag_total = frag_total_usize as u16;

        let frame_id = self.frame_counter;
        self.frame_counter = self.frame_counter.wrapping_add(1);

        let flags = if packet.is_keyframe { FLAG_KEYFRAME } else { 0 };

        chunks
            .into_iter()
            .enumerate()
            .map(|(i, chunk)| {
                #[allow(clippy::cast_possible_truncation)]
                let frag_index = i as u16;
                VideoFragment {
                    frame_id,
                    frag_index,
                    frag_total,
                    channel: Channel::Video,
                    flags,
                    payload: chunk.to_vec(),
                }
            })
            .collect()
    }

    /// Returns the current frame counter value (the next `frame_id` to be used).
    #[must_use]
    pub fn frame_counter(&self) -> u32 {
        self.frame_counter
    }

    /// Returns the configured maximum payload bytes per fragment.
    #[must_use]
    pub fn max_payload(&self) -> usize {
        self.max_payload
    }
}

#[cfg(test)]
mod tests;
