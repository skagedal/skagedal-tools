//! The commands: putting discovery, pairing, the server and the Companion
//! client together.

use std::io::{self, BufRead, Write};
use std::net::{IpAddr, Ipv4Addr, UdpSocket};
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use companion_link::client::Client;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use rand::RngExt;

use crate::discovery::{self, AppleTv};
use crate::pairings::{Pairing, Pairings};
use crate::server::{self, ServedFile, Site};

const SCAN_DURATION: Duration = Duration::from_secs(3);
const FIND_TIMEOUT: Duration = Duration::from_secs(5);
const DEVICE_TIMEOUT: Duration = Duration::from_secs(5);
/// What the Apple TV lists us as, under Remotes and Devices.
const CLIENT_NAME: &str = "skagedal-tools";

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
    let credentials = client.pair_finish(pin.trim(), CLIENT_NAME)?;

    let mut pairings = Pairings::load(&Pairings::default_path())?;
    pairings.insert(Pairing::new(&tv.name, &credentials));
    pairings.save()?;
    println!("Paired with {}.", tv.name);
    Ok(())
}

pub fn play(
    file: &Path,
    sub: Option<&Path>,
    device: Option<&str>,
    port: u16,
    url_only: bool,
) -> Result<()> {
    let mut files = vec![ServedFile::open(file)?];
    if let Some(sub) = sub {
        let sub = ServedFile::open(sub)?;
        if sub.file_name == files[0].file_name {
            bail!("the subtitle file has the same name as the movie");
        }
        files.push(sub);
    }
    let site = Site {
        token: format!("{:016x}", rand::rng().random::<u64>()),
        files,
    };

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
    let paths: Vec<(String, String)> = site
        .files
        .iter()
        .map(|file| (file.file_name.clone(), site.url_path(file)))
        .collect();
    let server = server::serve(site, port)?;

    let (movie_name, movie_path) = &paths[0];
    let sub = paths.get(1);
    let http_url = format!("http://{host}{movie_path}");
    let vlc_url = vlc_url(&host, movie_path, sub.map(|(_, path)| path.as_str()));

    println!("Serving: {movie_name}");
    println!("URL:     {http_url}");
    if let Some((sub_name, sub_path)) = sub {
        println!("Subs:    {sub_name}");
        println!("         http://{host}{sub_path}");
    }

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

/// The URL that has VLC play the movie served at `movie_path`. VLC's plain
/// `vlc://` handler takes only the movie, so subtitles go through its
/// x-callback-url handler, which reads `url` and `sub` query parameters.
fn vlc_url(host: &str, movie_path: &str, sub_path: Option<&str>) -> String {
    match sub_path {
        // VLC strips the vlc:// prefix and prepends http:// when no scheme
        // remains, so the scheme-less form is what its tvOS handler expects.
        None => format!("vlc://{host}{movie_path}"),
        // VLC splits the query on & before decoding, and decodes a value
        // only when it starts out encoded as http%3A%2F%2F, so each URL is
        // encoded whole -- the percent signs already in it included.
        Some(sub_path) => {
            let encode = |path: &str| {
                utf8_percent_encode(&format!("http://{host}{path}"), NON_ALPHANUMERIC).to_string()
            };
            format!(
                "vlc-x-callback://x-callback-url/stream?url={}&sub={}",
                encode(movie_path),
                encode(sub_path)
            )
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vlc_url_without_subtitles() {
        assert_eq!(
            vlc_url("10.0.0.2:8010", "/abc/A%20B.mkv", None),
            "vlc://10.0.0.2:8010/abc/A%20B.mkv"
        );
    }

    #[test]
    fn vlc_url_with_subtitles() {
        assert_eq!(
            vlc_url("10.0.0.2:8010", "/abc/A%20B.mkv", Some("/abc/A%20B.srt")),
            "vlc-x-callback://x-callback-url/stream\
             ?url=http%3A%2F%2F10%2E0%2E0%2E2%3A8010%2Fabc%2FA%2520B%2Emkv\
             &sub=http%3A%2F%2F10%2E0%2E0%2E2%3A8010%2Fabc%2FA%2520B%2Esrt"
        );
    }
}
