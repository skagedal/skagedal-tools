//! The commands: putting discovery, pairing, the server and the Companion
//! client together.

use std::io::{self, BufRead, Write};
use std::net::{IpAddr, Ipv4Addr, UdpSocket};
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use companion_link::client::Client;
use rand::RngExt;

use crate::discovery::{self, AppleTv};
use crate::pairings::{Pairing, Pairings};
use crate::server::{self, Movie};

const SCAN_DURATION: Duration = Duration::from_secs(3);
const FIND_TIMEOUT: Duration = Duration::from_secs(5);
const DEVICE_TIMEOUT: Duration = Duration::from_secs(5);
const CLIENT_NAME: &str = "appletv-vlc";

pub fn scan() -> Result<()> {
    let pairings = Pairings::load(&Pairings::default_path())?;
    let tvs = discovery::scan(SCAN_DURATION)?;
    if tvs.is_empty() {
        println!("No Apple TV found on the network.");
        return Ok(());
    }
    let width = tvs
        .iter()
        .map(|tv| tv.name.chars().count())
        .max()
        .unwrap_or(0);
    let model_width = tvs.iter().map(|tv| tv.model.len()).max().unwrap_or(0);
    for tv in tvs {
        let paired = if pairings.find(&tv.name).is_some() {
            "paired"
        } else {
            "not paired"
        };
        println!(
            "{:width$}   {:model_width$}   {:21}   {paired}",
            tv.name,
            tv.model,
            tv.address.to_string()
        );
    }
    Ok(())
}

pub fn pair(device: Option<&str>) -> Result<()> {
    let tv = match device {
        Some(name) => discovery::find(name, FIND_TIMEOUT)?
            .ok_or_else(|| anyhow!("no Apple TV named {name} on the network"))?,
        None => match discovery::scan(SCAN_DURATION)?.as_slice() {
            [only] => only.clone(),
            [] => bail!("no Apple TV found on the network"),
            several => bail!(
                "several Apple TVs on the network ({}); pass --device",
                several
                    .iter()
                    .map(|tv| tv.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        },
    };

    let mut client = Client::connect(tv.address, DEVICE_TIMEOUT)?;
    client.pair_start()?;
    print!("Enter the PIN shown on {}: ", tv.name);
    io::stdout().flush()?;
    let mut pin = String::new();
    io::stdin().lock().read_line(&mut pin)?;
    let credentials = client.pair_finish(pin.trim())?;

    let mut pairings = Pairings::load(&Pairings::default_path())?;
    pairings.insert(Pairing::new(&tv.name, &credentials));
    pairings.save()?;
    println!("Paired with {}.", tv.name);
    Ok(())
}

pub fn play(file: &Path, device: Option<&str>, port: u16, url_only: bool) -> Result<()> {
    let token = format!("{:016x}", rand::rng().random::<u64>());
    let movie = Movie::open(file, token)?;

    let target = if url_only {
        None
    } else {
        let pairings = Pairings::load(&Pairings::default_path())?;
        let pairing = pairings.select(device)?.clone();
        let tv = discovery::find(&pairing.name, FIND_TIMEOUT)?;
        Some((pairing, tv))
    };

    // The Apple TV fetches the URL, so it needs an address on the network
    // the TV is on.
    let ip = match &target {
        Some((_, Some(tv))) => route_to(tv.address.ip()),
        _ => lan_address(),
    }
    .context("could not determine this machine's LAN IP address")?;
    let host = format!("{ip}:{port}");
    let url_path = movie.url_path();
    let file_name = movie.file_name.clone();
    let server = server::serve(movie, port)?;

    let http_url = format!("http://{host}{url_path}");
    // VLC strips the vlc:// prefix and prepends http:// when no scheme
    // remains, so the scheme-less form is what its tvOS handler expects.
    let vlc_url = format!("vlc://{host}{url_path}");

    println!("Serving: {file_name}");
    println!("URL:     {http_url}");

    match target {
        None => {
            println!();
            println!("Enter it under Network Stream in VLC on the Apple TV.");
        }
        Some((pairing, tv)) => {
            println!("Opening in VLC on {} ...", pairing.name);
            if let Err(error) = launch(&pairing, tv.as_ref(), &vlc_url) {
                eprintln!();
                eprintln!("Launch failed: {error:#}");
                eprintln!();
                eprintln!("The stream is still up -- you can enter this by hand under Network");
                eprintln!("Stream in VLC:");
                eprintln!("  {http_url}");
            }
        }
    }

    println!();
    println!("Press Ctrl-C to stop serving.");
    server
        .join()
        .map_err(|_| anyhow!("the HTTP server stopped"))?;
    Ok(())
}

fn launch(pairing: &Pairing, tv: Option<&AppleTv>, url: &str) -> Result<()> {
    let tv =
        tv.ok_or_else(|| anyhow!("no Apple TV named {} answered on the network", pairing.name))?;
    let credentials = pairing.credentials()?;
    let mut client = Client::connect(tv.address, DEVICE_TIMEOUT)?;
    client.verify(&credentials).with_context(|| {
        format!(
            "{} did not accept our pairing; it may have been removed on the TV. Run `appletv-vlc pair --device \"{}\"`",
            pairing.name, pairing.name
        )
    })?;
    client.system_info(&credentials, CLIENT_NAME)?;
    client
        .open_url(url)
        .context("is VLC installed on the Apple TV?")?;
    Ok(())
}

/// The address of the interface this machine would use to reach `ip`. A UDP
/// connect sends nothing; it only picks the route.
fn route_to(ip: IpAddr) -> Result<IpAddr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    socket.connect((ip, 9))?;
    Ok(socket.local_addr()?.ip())
}

/// A private IPv4 address on an Ethernet or Wi-Fi interface, for when there
/// is no Apple TV to route to. The default route will not do: with a VPN up
/// it leads into the tunnel.
fn lan_address() -> Result<IpAddr> {
    let mut candidates: Vec<(String, Ipv4Addr)> = if_addrs::get_if_addrs()?
        .into_iter()
        .filter_map(|interface| match interface.ip() {
            IpAddr::V4(ip) if ip.is_private() && interface.name.starts_with("en") => {
                Some((interface.name, ip))
            }
            _ => None,
        })
        .collect();
    candidates.sort();
    candidates
        .first()
        .map(|(_, ip)| IpAddr::V4(*ip))
        .ok_or_else(|| anyhow!("no private IPv4 address on an en* interface"))
}
