//! The TLV8 pairing data carried in the `_pd` field of pairing frames.

use apple_opack::Value;
use hap_tlv8::Tlv8Map;
use hkdf::Hkdf;
use sha2::Sha512;

use crate::error::{Error, Refusal};

pub(crate) const STATE: u8 = 0x06;
pub(crate) const METHOD: u8 = 0x00;
pub(crate) const IDENTIFIER: u8 = 0x01;
pub(crate) const SALT: u8 = 0x02;
pub(crate) const PUBLIC_KEY: u8 = 0x03;
pub(crate) const PROOF: u8 = 0x04;
pub(crate) const ENCRYPTED_DATA: u8 = 0x05;
pub(crate) const ERROR: u8 = 0x07;
pub(crate) const SIGNATURE: u8 = 0x0A;
pub(crate) const NAME: u8 = 0x11;

/// The raw `_pd` bytes of a pairing message, after checking it for an error
/// code.
pub(crate) fn pairing_data(message: &Value) -> Result<Vec<u8>, Error> {
    let bytes = message
        .get("_pd")
        .and_then(Value::as_bytes)
        .ok_or(Error::Malformed("pairing message without pairing data"))?;
    if let Some(code) = Tlv8Map::parse(bytes)?.get(ERROR).and_then(<[u8]>::first) {
        return Err(Error::Refused(Refusal::from_code(*code)));
    }
    Ok(bytes.to_vec())
}

/// The `_pd` field holding `tlv`.
pub(crate) fn field(tlv: Vec<u8>) -> (Value, Value) {
    ("_pd".into(), Value::Bytes(tlv))
}

/// HKDF-SHA512 of a shared secret.
pub(crate) fn derive(secret: &[u8], salt: &[u8], info: &[u8]) -> [u8; 32] {
    let mut key = [0; 32];
    Hkdf::<Sha512>::new(Some(salt), secret)
        .expand(info, &mut key)
        .expect("32 bytes is a valid HKDF-SHA512 output length");
    key
}

/// An eight-byte label as a nonce, padded at the front.
pub(crate) fn label_nonce(label: &[u8; 8]) -> [u8; 12] {
    let mut nonce = [0; 12];
    nonce[4..].copy_from_slice(label);
    nonce
}
