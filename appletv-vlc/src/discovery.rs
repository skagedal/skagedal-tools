//! Finding Apple TVs by their Companion link Bonjour records.

use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use mdns_sd::{ServiceDaemon, ServiceEvent};

use crate::pairings::same_name;

const SERVICE_TYPE: &str = "_companion-link._tcp.local.";

/// An Apple TV heard on the network.
#[derive(Clone, Debug)]
pub struct AppleTv {
    pub name: String,
    pub model: String,
    pub address: SocketAddr,
}

/// Every Apple TV that answers within `duration`.
pub fn scan(duration: Duration) -> Result<Vec<AppleTv>> {
    let mut found = vec![];
    browse(duration, |tv| {
        if !found.iter().any(|known: &AppleTv| known.name == tv.name) {
            found.push(tv);
        }
        false
    })?;
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}

/// The Apple TV named `name`, if it answers within `timeout`.
pub fn find(name: &str, timeout: Duration) -> Result<Option<AppleTv>> {
    let mut found = None;
    browse(timeout, |tv| {
        let matches = same_name(&tv.name, name);
        if matches {
            found = Some(tv);
        }
        matches
    })?;
    Ok(found)
}

/// Browse until `duration` has passed or `found` returns true.
fn browse(duration: Duration, mut found: impl FnMut(AppleTv) -> bool) -> Result<()> {
    let daemon = ServiceDaemon::new().context("could not start mDNS")?;
    let receiver = daemon
        .browse(SERVICE_TYPE)
        .context("could not browse mDNS")?;
    let deadline = Instant::now() + duration;
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        let Ok(event) = receiver.recv_timeout(left) else {
            break;
        };
        let ServiceEvent::ServiceResolved(service) = event else {
            continue;
        };
        let model = service
            .get_property_val_str("rpMd")
            .unwrap_or_default()
            .to_string();
        if !model.starts_with("AppleTV") {
            continue;
        }
        let Some(ip) = service.get_addresses_v4().into_iter().min() else {
            continue;
        };
        let name = service
            .get_fullname()
            .strip_suffix(&format!(".{SERVICE_TYPE}"))
            .unwrap_or(service.get_fullname())
            .to_string();
        let tv = AppleTv {
            name,
            model,
            address: SocketAddr::new(IpAddr::V4(ip), service.port),
        };
        if found(tv) {
            break;
        }
    }
    let _ = daemon.shutdown();
    Ok(())
}
