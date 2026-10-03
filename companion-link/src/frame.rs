//! Companion frames: a one-byte type, a 24-bit big-endian length, a payload.

/// Length of a frame header.
pub const HEADER_LENGTH: usize = 4;

/// Largest payload a 24-bit length can describe.
pub const MAX_PAYLOAD_LENGTH: usize = 0xFF_FFFF;

/// The type byte of a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameType(pub u8);

impl FrameType {
    pub const PS_START: FrameType = FrameType(3);
    pub const PS_NEXT: FrameType = FrameType(4);
    pub const PV_START: FrameType = FrameType(5);
    pub const PV_NEXT: FrameType = FrameType(6);
    pub const U_OPACK: FrameType = FrameType(7);
    pub const E_OPACK: FrameType = FrameType(8);
    pub const P_OPACK: FrameType = FrameType(9);

    /// Whether frames of this type carry an OPACK payload.
    pub fn is_opack(self) -> bool {
        matches!(
            self,
            FrameType::PS_START
                | FrameType::PS_NEXT
                | FrameType::PV_START
                | FrameType::PV_NEXT
                | FrameType::U_OPACK
                | FrameType::E_OPACK
                | FrameType::P_OPACK
        )
    }
}

/// One frame, with its payload in the clear.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub frame_type: FrameType,
    pub payload: Vec<u8>,
}

/// The header for a frame of `frame_type` whose payload on the wire is
/// `length` bytes.
pub fn header(frame_type: FrameType, length: usize) -> [u8; HEADER_LENGTH] {
    let [_, a, b, c] = (length as u32).to_be_bytes();
    [frame_type.0, a, b, c]
}

/// Bytes read from the connection that do not yet make up a whole frame.
#[derive(Debug, Default)]
pub struct FrameBuffer {
    bytes: Vec<u8>,
}

impl FrameBuffer {
    /// Add bytes read from the connection.
    pub fn extend(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    /// Take the next whole frame, as its header and its payload as it was on
    /// the wire, if one has arrived.
    pub fn pop(&mut self) -> Option<([u8; HEADER_LENGTH], Vec<u8>)> {
        let header: [u8; HEADER_LENGTH] = self.bytes.get(..HEADER_LENGTH)?.try_into().ok()?;
        let length = u32::from_be_bytes([0, header[1], header[2], header[3]]) as usize;
        if self.bytes.len() < HEADER_LENGTH + length {
            return None;
        }
        let payload = self.bytes[HEADER_LENGTH..HEADER_LENGTH + length].to_vec();
        self.bytes.drain(..HEADER_LENGTH + length);
        Some((header, payload))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_has_big_endian_24_bit_length() {
        assert_eq!(header(FrameType::E_OPACK, 0x01_0203), [8, 1, 2, 3]);
    }

    #[test]
    fn buffer_waits_for_whole_frame() {
        let mut buffer = FrameBuffer::default();
        buffer.extend(&[8, 0, 0, 3, b'a']);
        assert_eq!(buffer.pop(), None);
        buffer.extend(&[b'b', b'c', 7, 0]);
        assert_eq!(buffer.pop(), Some(([8, 0, 0, 3], b"abc".to_vec())));
        assert_eq!(buffer.pop(), None);
        buffer.extend(&[0, 0]);
        assert_eq!(buffer.pop(), Some(([7, 0, 0, 0], vec![])));
    }
}
