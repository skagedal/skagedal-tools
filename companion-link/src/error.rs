//! Errors from the Companion protocol.

use thiserror::Error;

/// Why a Companion exchange failed.
#[derive(Debug, Error)]
pub enum Error {
    #[error("Companion connection was closed by the device")]
    Closed,
    #[error("Companion frame could not be decrypted")]
    Decrypt,
    #[error("Companion message is malformed: {0}")]
    Malformed(&'static str),
    #[error("Companion message could not be encoded or decoded: {0}")]
    Opack(#[from] apple_opack::Error),
    #[error("Companion pairing data is malformed: {0}")]
    Tlv(#[from] hap_tlv8::Tlv8Error),
    #[error("Companion pairing failed: {0}")]
    Crypto(#[from] hap_crypto::CryptoError),
    #[error("Companion pairing was refused by the device: {0}")]
    Refused(Refusal),
    #[error("Companion request {identifier} failed: {message}")]
    Request { identifier: String, message: String },
    #[error("Companion coroutine was resumed after completing")]
    Finished,
}

/// The error codes a device puts in a pairing reply, from the HomeKit
/// Accessory Protocol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    Authentication,
    BackOff,
    MaxPeers,
    MaxTries,
    Unavailable,
    Busy,
    Unknown(u8),
}

impl Refusal {
    /// The refusal for a TLV8 error code.
    pub fn from_code(code: u8) -> Self {
        match code {
            0x02 => Refusal::Authentication,
            0x03 => Refusal::BackOff,
            0x04 => Refusal::MaxPeers,
            0x05 => Refusal::MaxTries,
            0x06 => Refusal::Unavailable,
            0x07 => Refusal::Busy,
            other => Refusal::Unknown(other),
        }
    }
}

impl core::fmt::Display for Refusal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Refusal::Authentication => write!(f, "wrong PIN, or the pairing is no longer known"),
            Refusal::BackOff => write!(f, "too many attempts; try again later"),
            Refusal::MaxPeers => write!(f, "no room for more paired devices"),
            Refusal::MaxTries => write!(f, "too many failed attempts"),
            Refusal::Unavailable => write!(f, "pairing is not available"),
            Refusal::Busy => write!(f, "busy pairing with another device"),
            Refusal::Unknown(code) => write!(f, "error code {code:#04x}"),
        }
    }
}
