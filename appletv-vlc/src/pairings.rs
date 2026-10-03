//! The Apple TVs we have paired with, and the keys for each.

use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use companion_link::credentials::Credentials;
use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
struct PairingsFile {
    #[serde(default, rename = "device")]
    devices: Vec<Pairing>,
}

/// One paired Apple TV.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Pairing {
    pub name: String,
    pub device_id: String,
    pub device_public_key: String,
    pub client_id: String,
    pub client_secret_key: String,
}

impl Pairing {
    pub fn new(name: &str, credentials: &Credentials) -> Self {
        Pairing {
            name: name.to_string(),
            device_id: credentials.device_id.clone(),
            device_public_key: hex(&credentials.device_public_key),
            client_id: credentials.client_id.clone(),
            client_secret_key: hex(&credentials.client_secret_key),
        }
    }

    pub fn credentials(&self) -> Result<Credentials> {
        Ok(Credentials {
            device_id: self.device_id.clone(),
            device_public_key: unhex(&self.device_public_key).context("bad device_public_key")?,
            client_id: self.client_id.clone(),
            client_secret_key: unhex(&self.client_secret_key).context("bad client_secret_key")?,
        })
    }
}

/// The pairings file.
pub struct Pairings {
    path: PathBuf,
    devices: Vec<Pairing>,
}

impl Pairings {
    pub fn default_path() -> PathBuf {
        skagedal_dirs::data_dir("appletv-vlc").join("pairings.toml")
    }

    pub fn load(path: &Path) -> Result<Self> {
        let devices = match fs::read_to_string(path) {
            Ok(text) => {
                toml::from_str::<PairingsFile>(&text)
                    .with_context(|| format!("could not read {}", path.display()))?
                    .devices
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => vec![],
            Err(error) => {
                return Err(error).with_context(|| format!("could not read {}", path.display()));
            }
        };
        Ok(Pairings {
            path: path.to_path_buf(),
            devices,
        })
    }

    pub fn find(&self, name: &str) -> Option<&Pairing> {
        self.devices
            .iter()
            .find(|pairing| same_name(&pairing.name, name))
    }

    /// The pairing named `name`, or the only one there is.
    pub fn select(&self, name: Option<&str>) -> Result<&Pairing> {
        match (name, self.devices.as_slice()) {
            (Some(name), _) => match self.find(name) {
                Some(pairing) => Ok(pairing),
                None => bail!("not paired with {name}; run `appletv-vlc pair --device \"{name}\"`"),
            },
            (None, [only]) => Ok(only),
            (None, []) => {
                bail!(
                    "not paired with any Apple TV; run `appletv-vlc pair` (see `appletv-vlc scan`)"
                )
            }
            (None, _) => bail!(
                "paired with several Apple TVs ({}); pass --device or set APPLETV_VLC_DEVICE",
                self.devices
                    .iter()
                    .map(|pairing| pairing.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }

    /// Add `pairing`, replacing any earlier one with the same name.
    pub fn insert(&mut self, pairing: Pairing) {
        self.devices
            .retain(|existing| !same_name(&existing.name, &pairing.name));
        self.devices.push(pairing);
    }

    /// Write the file, readable by its owner only since it holds private
    /// keys.
    pub fn save(&self) -> Result<()> {
        let directory = self.path.parent().context("pairings path has no parent")?;
        fs::create_dir_all(directory)
            .with_context(|| format!("could not create {}", directory.display()))?;
        let text = toml::to_string(&PairingsFile {
            devices: self.devices.clone(),
        })?;
        let temporary = self.path.with_extension("toml.tmp");
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temporary)
            .with_context(|| format!("could not write {}", temporary.display()))?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, &self.path)
            .with_context(|| format!("could not write {}", self.path.display()))?;
        Ok(())
    }
}

pub fn same_name(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex(text: &str) -> Result<[u8; 32]> {
    if text.len() != 64 {
        bail!("expected 64 hex digits, got {}", text.len());
    }
    let mut bytes = [0; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16)?;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn credentials() -> Credentials {
        Credentials {
            device_id: "E3E25DF5-AB48-40F0-B61D-2AADADAB0FBD".into(),
            device_public_key: [0xAB; 32],
            client_id: "01c71d5d-14b3-4486-877a-60d9304510d6".into(),
            client_secret_key: [0x01; 32],
        }
    }

    #[test]
    fn round_trips_through_the_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested/pairings.toml");
        let mut pairings = Pairings::load(&path).unwrap();
        pairings.insert(Pairing::new("Vardagsrum", &credentials()));
        pairings.save().unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        let loaded = Pairings::load(&path).unwrap();
        assert_eq!(
            loaded.select(None).unwrap().credentials().unwrap(),
            credentials()
        );
        assert_eq!(
            loaded.select(Some("VARDAGSRUM")).unwrap().name,
            "Vardagsrum"
        );
    }

    #[test]
    fn insert_replaces_by_name() {
        let directory = tempfile::tempdir().unwrap();
        let mut pairings = Pairings::load(&directory.path().join("pairings.toml")).unwrap();
        pairings.insert(Pairing::new("Vardagsrum", &credentials()));
        pairings.insert(Pairing::new("vardagsrum", &credentials()));
        assert_eq!(pairings.devices.len(), 1);
    }

    #[test]
    fn select_needs_a_name_with_several() {
        let directory = tempfile::tempdir().unwrap();
        let mut pairings = Pairings::load(&directory.path().join("pairings.toml")).unwrap();
        assert!(pairings.select(None).is_err());
        pairings.insert(Pairing::new("Vardagsrum", &credentials()));
        pairings.insert(Pairing::new("Sovrum", &credentials()));
        assert!(pairings.select(None).is_err());
        assert!(pairings.select(Some("Kök")).is_err());
        assert_eq!(pairings.select(Some("sovrum")).unwrap().name, "Sovrum");
    }
}
