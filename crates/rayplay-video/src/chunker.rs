use crate::packet::EncodedPacket;

/// Default maximum bytes per network chunk.
///
/// Set to 1200 bytes to stay comfortably below the 1280-byte IPv6 MTU
/// and typical 1500-byte Ethernet MTU (accounting for UDP/IP headers).
pub const DEFAULT_CHUNK_SIZE: usize = 1200;

/// A single network-ready piece of an encoded frame.
///
/// Large encoded packets are split into multiple `NetworkChunk`s so that each
/// UDP datagram fits within the network MTU. The receiver reassembles chunks
/// back into an `EncodedPacket` using `packet_index`, `chunk_index`, and
/// `total_chunks`.
#[derive(Debug, Clone)]
pub struct NetworkChunk {
    /// Chunk payload bytes (≤ `max_chunk_size`).
    pub data: Vec<u8>,
    /// Monotonically increasing counter for the source encoded packet.
    pub packet_index: u32,
    /// Zero-based index of this chunk within the packet.
    pub chunk_index: u16,
    /// Total number of chunks for this packet.
    pub total_chunks: u16,
    /// Propagated from the source packet — true if packet is an IDR frame.
    pub is_keyframe: bool,
    /// Presentation timestamp of the source frame in microseconds.
    pub timestamp_us: u64,
}

/// Splits `EncodedPacket`s into `NetworkChunk`s sized for UDP transmission.
pub struct FrameChunker {
    max_chunk_size: usize,
    packet_counter: u32,
}

impl FrameChunker {
    /// Creates a `FrameChunker` with a configurable maximum chunk size.
    ///
    /// # Panics
    ///
    /// Panics if `max_chunk_size` is zero.
    #[must_use]
    pub fn new(max_chunk_size: usize) -> Self {
        assert!(
            max_chunk_size > 0,
            "max_chunk_size must be greater than zero"
        );
        Self {
            max_chunk_size,
            packet_counter: 0,
        }
    }

    /// Creates a `FrameChunker` with the default MTU-friendly chunk size.
    #[must_use]
    pub fn with_default_chunk_size() -> Self {
        Self::new(DEFAULT_CHUNK_SIZE)
    }

    /// Splits an encoded packet into one or more network chunks.
    ///
    /// Returns an empty `Vec` if `packet.data` is empty.
    /// The internal packet counter increments (wrapping) after each call.
    ///
    /// # Panics
    ///
    /// Panics if `packet.data.len() / max_chunk_size` exceeds `u16::MAX` (65535).
    /// This cannot occur with any realistic packet size and the default 1200-byte
    /// chunk size (max packet ≈ 78 MB before panic).
    pub fn chunk(&mut self, packet: &EncodedPacket) -> Vec<NetworkChunk> {
        if packet.data.is_empty() {
            return vec![];
        }

        let raw_chunks: Vec<&[u8]> = packet.data.chunks(self.max_chunk_size).collect();
        let total_chunks = u16::try_from(raw_chunks.len())
            .expect("packet produces more than 65535 chunks — reduce max_chunk_size");
        let packet_index = self.packet_counter;
        self.packet_counter = self.packet_counter.wrapping_add(1);

        raw_chunks
            .into_iter()
            .enumerate()
            .map(|(i, slice)| NetworkChunk {
                data: slice.to_vec(),
                packet_index,
                chunk_index: u16::try_from(i).expect("chunk index exceeds u16::MAX"),
                total_chunks,
                is_keyframe: packet.is_keyframe,
                timestamp_us: packet.timestamp_us,
            })
            .collect()
    }

    /// Returns the current packet counter value (next packet index to be assigned).
    #[must_use]
    pub fn packet_counter(&self) -> u32 {
        self.packet_counter
    }
}

#[cfg(test)]
mod tests;
