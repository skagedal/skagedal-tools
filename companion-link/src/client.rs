//! A blocking client running the coroutines over a TCP connection.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use apple_opack::Value;
use rand::RngExt;
use thiserror::Error;

use crate::coroutine::{Coroutine, Reply, State, Wants};
use crate::credentials::Credentials;
use crate::pair_setup::{PairSetupFinish, PairSetupPending, PairSetupStart};
use crate::pair_verify::PairVerify;
use crate::request::Request;
use crate::session::Session;

/// Why a client operation failed.
#[derive(Debug, Error)]
pub enum Error {
    #[error("Companion connection failed: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Protocol(#[from] crate::error::Error),
    #[error("Companion connection is unusable after an earlier error")]
    Unusable,
    #[error("Companion pair setup was not started")]
    NotPairing,
}

/// A connection to one device.
pub struct Client {
    stream: TcpStream,
    session: Option<Session>,
    pending: Option<PairSetupPending>,
}

impl Client {
    /// Connect to `address`, giving up on any read or write that takes
    /// longer than `timeout`.
    pub fn connect(address: SocketAddr, timeout: Duration) -> Result<Self, Error> {
        let stream = TcpStream::connect_timeout(&address, timeout)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        stream.set_nodelay(true)?;
        Ok(Client {
            stream,
            session: Some(Session::new(rand::rng().random())),
            pending: None,
        })
    }

    /// Ask the device to show a PIN for pairing.
    pub fn pair_start(&mut self) -> Result<(), Error> {
        let session = self.session.take().ok_or(Error::Unusable)?;
        self.pending = Some(drive(&mut self.stream, PairSetupStart::new(session))?);
        Ok(())
    }

    /// Finish pairing with the PIN the device shows, returning the
    /// credentials to keep.
    pub fn pair_finish(&mut self, pin: &str) -> Result<Credentials, Error> {
        let pending = self.pending.take().ok_or(Error::NotPairing)?;
        let mut rng = rand::rng();
        let client_id = format_uuid(rng.random());
        let pair = PairSetupFinish::new(pending, pin, client_id, rng.random(), rng.random());
        drive(&mut self.stream, pair)
    }

    /// Prove we are paired, and encrypt the connection from here on.
    pub fn verify(&mut self, credentials: &Credentials) -> Result<(), Error> {
        let session = self.session.take().ok_or(Error::Unusable)?;
        let verify = PairVerify::new(session, credentials.clone(), rand::rng().random());
        self.session = Some(drive(&mut self.stream, verify)?);
        Ok(())
    }

    /// Send a request, returning the response content.
    pub fn request(&mut self, identifier: &str, content: Value) -> Result<Value, Error> {
        let session = self.session.take().ok_or(Error::Unusable)?;
        let (session, response) =
            drive(&mut self.stream, Request::new(session, identifier, content))?;
        self.session = Some(session);
        Ok(response)
    }

    /// Tell the device who we are; it may refuse other requests until this
    /// has been sent.
    pub fn system_info(&mut self, credentials: &Credentials, name: &str) -> Result<(), Error> {
        let content = Value::Dict(vec![
            ("_bf".into(), Value::int(0)),
            ("_cf".into(), Value::int(512)),
            ("_clFl".into(), Value::int(128)),
            (
                "_i".into(),
                credentials.client_id.replace('-', "")[..12].into(),
            ),
            (
                "_idsID".into(),
                credentials.client_id.as_bytes().to_vec().into(),
            ),
            ("_pubID".into(), credentials.client_id.as_str().into()),
            ("_sf".into(), Value::int(256)),
            ("_sv".into(), "170.18".into()),
            ("model".into(), "iPhone10,6".into()),
            ("name".into(), name.into()),
        ]);
        self.request("_systemInfo", content)?;
        Ok(())
    }

    /// Open the app with `bundle_id`.
    pub fn launch_app(&mut self, bundle_id: &str) -> Result<(), Error> {
        let content = Value::Dict(vec![("_bundleID".into(), bundle_id.into())]);
        self.request("_launchApp", content)?;
        Ok(())
    }

    /// Open `url` in whichever app handles its scheme.
    pub fn open_url(&mut self, url: &str) -> Result<(), Error> {
        let content = Value::Dict(vec![("_urlS".into(), url.into())]);
        self.request("_launchApp", content)?;
        Ok(())
    }
}

/// Run `coroutine` to completion over `stream`.
pub fn drive<C: Coroutine>(stream: &mut TcpStream, mut coroutine: C) -> Result<C::Output, Error> {
    let mut buffer = vec![0; 64 * 1024];
    let mut last: Option<Option<usize>> = None;
    loop {
        let arg = match last {
            None => None,
            Some(None) => Some(Reply::Wrote),
            Some(Some(read)) => Some(Reply::Read(&buffer[..read])),
        };
        match coroutine.resume(arg) {
            State::Yielded(Wants::Write(bytes)) => {
                stream.write_all(&bytes)?;
                last = Some(None);
            }
            State::Yielded(Wants::Read) => {
                last = Some(Some(stream.read(&mut buffer)?));
            }
            State::Complete(result) => return Ok(result?),
        }
    }
}

fn format_uuid(bytes: [u8; 16]) -> String {
    let mut bytes = bytes;
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}
