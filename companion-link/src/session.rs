//! The state of one connection, carried from coroutine to coroutine.

use apple_opack::{Value, pack, unpack};
use log::trace;

use crate::cipher::{Cipher, TAG_LENGTH};
use crate::error::Error;
use crate::frame::{Frame, FrameBuffer, FrameType, MAX_PAYLOAD_LENGTH, header};

/// One Companion connection: bytes read but not yet framed, the session
/// cipher once pair verify has set one up, and the next transaction id.
#[derive(Debug)]
pub struct Session {
    buffer: FrameBuffer,
    cipher: Option<Cipher>,
    next_xid: u64,
}

impl Session {
    /// A fresh, unencrypted connection. Transaction ids count up from
    /// `first_xid`, which the caller should pick at random.
    pub fn new(first_xid: u16) -> Self {
        Session {
            buffer: FrameBuffer::default(),
            cipher: None,
            next_xid: first_xid.into(),
        }
    }

    /// Whether pair verify has set up encryption.
    pub fn is_encrypted(&self) -> bool {
        self.cipher.is_some()
    }

    pub(crate) fn enable_encryption(&mut self, out_key: [u8; 32], in_key: [u8; 32]) {
        self.cipher = Some(Cipher::new(out_key, in_key));
    }

    pub(crate) fn take_xid(&mut self) -> u64 {
        let xid = self.next_xid;
        self.next_xid += 1;
        xid
    }

    /// Encode `message` as a frame of `frame_type`, encrypted if the session
    /// is.
    pub(crate) fn encode(
        &mut self,
        frame_type: FrameType,
        message: &Value,
    ) -> Result<Vec<u8>, Error> {
        trace!("send {frame_type:?}: {message:?}");
        let payload = pack(message)?;
        let encrypted = self.cipher.is_some() && !payload.is_empty();
        let length = payload.len() + if encrypted { TAG_LENGTH } else { 0 };
        if length > MAX_PAYLOAD_LENGTH {
            return Err(Error::Malformed("message too large for a frame"));
        }
        let header = header(frame_type, length);
        let mut bytes = header.to_vec();
        match &mut self.cipher {
            Some(cipher) if encrypted => bytes.extend(cipher.encrypt(&header, &payload)?),
            _ => bytes.extend(payload),
        }
        Ok(bytes)
    }

    pub(crate) fn feed(&mut self, bytes: &[u8]) {
        self.buffer.extend(bytes);
    }

    /// The next whole frame read, decrypted.
    pub(crate) fn next_frame(&mut self) -> Result<Option<Frame>, Error> {
        let Some((header, payload)) = self.buffer.pop() else {
            return Ok(None);
        };
        let payload = match &mut self.cipher {
            Some(cipher) if !payload.is_empty() => cipher.decrypt(&header, &payload)?,
            _ => payload,
        };
        Ok(Some(Frame {
            frame_type: FrameType(header[0]),
            payload,
        }))
    }

    /// The next whole frame read, decoded as OPACK, skipping frames that do
    /// not carry OPACK.
    pub(crate) fn next_message(&mut self) -> Result<Option<(FrameType, Value)>, Error> {
        while let Some(frame) = self.next_frame()? {
            if !frame.frame_type.is_opack() {
                trace!("skip {:?} frame", frame.frame_type);
                continue;
            }
            let (message, _) = unpack(&frame.payload)?;
            trace!("receive {:?}: {message:?}", frame.frame_type);
            return Ok(Some((frame.frame_type, message)));
        }
        Ok(None)
    }
}
