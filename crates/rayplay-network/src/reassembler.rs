//! Reassembles [`VideoFragment`]s back into [`EncodedPacket`]s.

use std::collections::HashMap;

use rayplay_core::packet::EncodedPacket;

use crate::wire::{FLAG_KEYFRAME, VideoFragment};

/// Maximum number of incomplete frames held in memory simultaneously (ADR-003).
pub const MAX_IN_FLIGHT_FRAMES: usize = 4;

/// State for a single partially-received frame.
struct PendingFrame {
    frag_total: u16,
    is_keyframe: bool,
    fragments: Vec<Option<Vec<u8>>>,
    received: u16,
}

impl PendingFrame {
    fn new(frag_total: u16, is_keyframe: bool) -> Self {
        Self {
            frag_total,
            is_keyframe,
            fragments: vec![None; usize::from(frag_total)],
            received: 0,
        }
    }

    /// Returns `true` if all fragments have been received.
    fn is_complete(&self) -> bool {
        self.received == self.frag_total
    }

    /// Assembles all fragments into a contiguous payload in order.
    fn assemble(self) -> Vec<u8> {
        self.fragments.into_iter().flatten().flatten().collect()
    }
}

/// Reassembles [`VideoFragment`]s into complete [`EncodedPacket`]s.
///
/// Bounded to at most `max_pending` incomplete frames at a time. When a new
/// `frame_id` arrives and the buffer is full, the oldest incomplete frame is
/// evicted (dropped) to make room — matching the ADR-003 "drop oldest" policy.
pub struct VideoReassembler {
    pending: HashMap<u32, PendingFrame>,
    max_pending: usize,
}

impl VideoReassembler {
    /// Creates a new reassembler with the given maximum number of in-flight frames.
    ///
    /// # Panics
    ///
    /// Panics if `max_pending` is zero.
    #[must_use]
    pub fn new(max_pending: usize) -> Self {
        assert!(max_pending > 0, "max_pending must be > 0");
        Self {
            pending: HashMap::new(),
            max_pending,
        }
    }

    /// Creates a reassembler using [`MAX_IN_FLIGHT_FRAMES`] (4).
    #[must_use]
    pub fn with_default_max() -> Self {
        Self::new(MAX_IN_FLIGHT_FRAMES)
    }

    /// Ingests one fragment.
    ///
    /// Returns `Some(EncodedPacket)` when all fragments for a frame arrive;
    /// `None` otherwise.
    ///
    /// # Drop / eviction semantics
    ///
    /// - When the buffer is full and a fragment for a *new* `frame_id` arrives,
    ///   the frame with the lowest `frame_id` is evicted.
    /// - Duplicate fragments (slot already filled) are silently ignored.
    /// - Fragments whose `frag_index >= existing frag_total` are silently ignored
    ///   (inconsistent sender — protect against out-of-bounds).
    pub fn ingest(&mut self, frag: VideoFragment) -> Option<EncodedPacket> {
        let frame_id = frag.frame_id;

        // If this frame_id isn't yet tracked and we're at capacity, evict oldest.
        if !self.pending.contains_key(&frame_id) {
            if self.pending.len() >= self.max_pending {
                self.evict_oldest();
            }
            let is_keyframe = frag.flags & FLAG_KEYFRAME != 0;
            self.pending
                .insert(frame_id, PendingFrame::new(frag.frag_total, is_keyframe));
        }

        let entry = self.pending.get_mut(&frame_id)?;

        // Guard against inconsistent frag_index vs the stored frag_total.
        let idx = usize::from(frag.frag_index);
        if idx >= entry.fragments.len() {
            return None;
        }

        // Ignore duplicate fragments.
        if entry.fragments[idx].is_some() {
            return None;
        }

        entry.fragments[idx] = Some(frag.payload);
        entry.received += 1;

        if entry.is_complete() {
            let frame = self.pending.remove(&frame_id)?;
            let is_keyframe = frame.is_keyframe;
            let data = frame.assemble();
            Some(EncodedPacket::new(data, is_keyframe, 0, 0))
        } else {
            None
        }
    }

    /// Evicts all incomplete frames whose `frame_id` is strictly less than
    /// `before_frame_id`.
    ///
    /// Returns the number of frames evicted.
    pub fn evict_before(&mut self, before_frame_id: u32) -> usize {
        let keys_to_remove: Vec<u32> = self
            .pending
            .keys()
            .copied()
            .filter(|&k| k < before_frame_id)
            .collect();
        let count = keys_to_remove.len();
        for k in keys_to_remove {
            self.pending.remove(&k);
        }
        count
    }

    /// Returns the number of frames currently buffered (incomplete).
    #[must_use]
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Evicts the frame with the smallest `frame_id` (oldest in-flight frame).
    fn evict_oldest(&mut self) {
        if let Some(&oldest) = self.pending.keys().min() {
            self.pending.remove(&oldest);
        }
    }
}

#[cfg(test)]
mod tests;
