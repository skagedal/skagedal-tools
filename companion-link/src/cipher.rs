//! ChaCha20-Poly1305 for an established session.

use hap_crypto::aead::{chacha20poly1305_open, chacha20poly1305_seal};

use crate::error::Error;

/// Length of the Poly1305 tag added to every encrypted payload.
pub const TAG_LENGTH: usize = 16;

/// Session encryption, one key and nonce counter per direction.
///
/// The nonce is the message counter as 12 little-endian bytes, and the frame
/// header is the associated data.
pub struct Cipher {
    out_key: [u8; 32],
    in_key: [u8; 32],
    out_counter: u64,
    in_counter: u64,
}

impl Cipher {
    pub fn new(out_key: [u8; 32], in_key: [u8; 32]) -> Self {
        Cipher {
            out_key,
            in_key,
            out_counter: 0,
            in_counter: 0,
        }
    }

    /// Encrypt the next outgoing payload.
    pub fn encrypt(&mut self, aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, Error> {
        let nonce = nonce(self.out_counter);
        self.out_counter += 1;
        chacha20poly1305_seal(&self.out_key, &nonce, aad, plaintext).map_err(|_| Error::Decrypt)
    }

    /// Decrypt the next incoming payload.
    pub fn decrypt(&mut self, aad: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, Error> {
        let nonce = nonce(self.in_counter);
        self.in_counter += 1;
        chacha20poly1305_open(&self.in_key, &nonce, aad, ciphertext).map_err(|_| Error::Decrypt)
    }
}

impl core::fmt::Debug for Cipher {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Cipher").finish_non_exhaustive()
    }
}

fn nonce(counter: u64) -> [u8; 12] {
    let mut nonce = [0; 12];
    nonce[..8].copy_from_slice(&counter.to_le_bytes());
    nonce
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_with_swapped_keys() {
        let mut client = Cipher::new([1; 32], [2; 32]);
        let mut device = Cipher::new([2; 32], [1; 32]);
        for message in [&b"first"[..], b"second"] {
            let sealed = client.encrypt(b"aad", message).unwrap();
            assert_eq!(sealed.len(), message.len() + TAG_LENGTH);
            assert_eq!(device.decrypt(b"aad", &sealed).unwrap(), message);
        }
    }

    #[test]
    fn rejects_wrong_aad() {
        let mut client = Cipher::new([1; 32], [2; 32]);
        let mut device = Cipher::new([2; 32], [1; 32]);
        let sealed = client.encrypt(b"aad", b"hello").unwrap();
        assert!(device.decrypt(b"other", &sealed).is_err());
    }
}
