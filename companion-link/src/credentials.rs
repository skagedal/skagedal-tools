//! What pairing leaves behind, and pair verify needs.

/// The keys and identifiers from pairing with one device.
#[derive(Clone, PartialEq, Eq)]
pub struct Credentials {
    /// The device's pairing identifier.
    pub device_id: String,
    /// The device's long-term Ed25519 public key.
    pub device_public_key: [u8; 32],
    /// Our pairing identifier with this device.
    pub client_id: String,
    /// The seed of our long-term Ed25519 key with this device.
    pub client_secret_key: [u8; 32],
}

impl core::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Credentials")
            .field("device_id", &self.device_id)
            .field("client_id", &self.client_id)
            .finish_non_exhaustive()
    }
}
