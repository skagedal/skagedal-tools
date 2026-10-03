//! Pair setup: trade a PIN shown on the device for long-term keys.
//!
//! Split in two where the device shows its PIN: [`PairSetupStart`] asks for
//! it, and [`PairSetupFinish`] continues with the PIN the user typed.

use apple_opack::{Value, pack};
use hap_crypto::aead::{chacha20poly1305_open, chacha20poly1305_seal};
use hap_crypto::{ControllerKeypair, verify_ed25519};
use hap_tlv8::{Tlv8Map, Tlv8Writer};
use sha2::{Digest, Sha512};
use srp::Group;
use srp::bigint::BoxedUint;
use srp::groups::G3072;

use crate::coroutine::{Coroutine, Reply, State};
use crate::credentials::Credentials;
use crate::error::Error;
use crate::exchange::Exchange;
use crate::frame::FrameType;
use crate::pairing_data::{
    ENCRYPTED_DATA, IDENTIFIER, METHOD, NAME, PROOF, PUBLIC_KEY, SALT, SIGNATURE, STATE, derive,
    field, label_nonce, pairing_data,
};
use crate::session::Session;

/// Pair setup up to the point where the device shows a PIN.
#[derive(Debug)]
pub struct PairSetupStart {
    session: Option<Session>,
    exchange: Result<Exchange, Option<Error>>,
}

/// A pair setup waiting for its PIN.
#[derive(Debug)]
pub struct PairSetupPending {
    session: Session,
    m2: Vec<u8>,
}

impl PairSetupStart {
    pub fn new(mut session: Session) -> Self {
        let mut m1 = Vec::new();
        let mut tlv = Tlv8Writer::new(&mut m1);
        tlv.push_u8(METHOD, 0);
        tlv.push_u8(STATE, 1);
        let exchange = Exchange::auth(&mut session, FrameType::PS_START, fields(m1));
        PairSetupStart {
            session: Some(session),
            exchange: exchange.map_err(Some),
        }
    }

    fn step(&mut self, arg: Option<Reply<'_>>) -> Result<State<PairSetupPending>, Error> {
        let exchange = self
            .exchange
            .as_mut()
            .map_err(|error| error.take().unwrap_or(Error::Finished))?;
        let session = self.session.as_mut().ok_or(Error::Finished)?;
        let message = match exchange.resume(session, arg) {
            State::Complete(result) => result?,
            State::Yielded(wants) => return Ok(State::Yielded(wants)),
        };
        let m2 = pairing_data(&message)?;
        let session = self.session.take().ok_or(Error::Finished)?;
        Ok(State::Complete(Ok(PairSetupPending { session, m2 })))
    }
}

impl Coroutine for PairSetupStart {
    type Output = PairSetupPending;

    fn resume(&mut self, arg: Option<Reply<'_>>) -> State<PairSetupPending> {
        self.step(arg)
            .unwrap_or_else(|error| State::Complete(Err(error)))
    }
}

/// Pair setup from the PIN on: SRP-6a over the PIN (M3, M4), then an
/// exchange of long-term keys encrypted under the SRP session key (M5, M6).
///
/// This is the HomeKit pair setup, as `hap_crypto::PairSetupClient` does it,
/// with one addition: M5 carries a name for the device to list us under.
pub struct PairSetupFinish {
    session: Session,
    keypair: ControllerKeypair,
    name: String,
    exchange: Option<Exchange>,
    phase: Phase,
    error: Option<Error>,
}

enum Phase {
    /// M3 sent, waiting for M4.
    AwaitM4(Srp),
    /// M5 sent, waiting for M6; holds the SRP session key.
    AwaitM6(Vec<u8>),
}

/// The SRP-6a username HomeKit uses.
const USERNAME: &[u8] = b"Pair-Setup";

impl PairSetupFinish {
    /// Continue `pending` with the `pin` shown on the device. The device
    /// lists us as `name`.
    ///
    /// `client_id` and `client_secret_key` become our identity with this
    /// device, and `srp_private` is the secret SRP exponent; all three should
    /// be fresh and random.
    pub fn new(
        pending: PairSetupPending,
        pin: &str,
        name: &str,
        client_id: String,
        client_secret_key: [u8; 32],
        srp_private: [u8; 32],
    ) -> Self {
        let PairSetupPending { mut session, m2 } = pending;
        let mut this = PairSetupFinish {
            keypair: ControllerKeypair::from_seed(client_id, client_secret_key),
            name: name.to_string(),
            exchange: None,
            phase: Phase::AwaitM6(vec![]),
            error: None,
            session: Session::new(0),
        };
        let started = Srp::new(pin, &m2, &srp_private).and_then(|srp| {
            let mut m3 = Vec::new();
            let mut tlv = Tlv8Writer::new(&mut m3);
            tlv.push_u8(STATE, 3);
            tlv.push(PUBLIC_KEY, &srp.a_pub);
            tlv.push(PROOF, &srp.m1);
            let exchange = Exchange::auth(&mut session, FrameType::PS_NEXT, fields(m3))?;
            Ok((srp, exchange))
        });
        match started {
            Ok((srp, exchange)) => {
                this.exchange = Some(exchange);
                this.phase = Phase::AwaitM4(srp);
            }
            Err(error) => this.error = Some(error),
        }
        this.session = session;
        this
    }

    fn step(&mut self, mut arg: Option<Reply<'_>>) -> Result<State<Credentials>, Error> {
        if let Some(error) = self.error.take() {
            return Err(error);
        }
        loop {
            let exchange = self.exchange.as_mut().ok_or(Error::Finished)?;
            let message = match exchange.resume(&mut self.session, arg.take()) {
                State::Complete(result) => result?,
                State::Yielded(wants) => return Ok(State::Yielded(wants)),
            };
            let tlv = Tlv8Map::parse(&pairing_data(&message)?)?;
            match &self.phase {
                Phase::AwaitM4(srp) => {
                    if tlv.get(PROOF) != Some(&srp.m2[..]) {
                        return Err(Error::Malformed("pair setup M4 proof does not match"));
                    }
                    let key = srp.key.clone();
                    let m5 = self.m5(&key)?;
                    self.exchange = Some(Exchange::auth(
                        &mut self.session,
                        FrameType::PS_NEXT,
                        fields(m5),
                    )?);
                    self.phase = Phase::AwaitM6(key);
                }
                Phase::AwaitM6(key) => {
                    let credentials = self.handle_m6(key, &tlv)?;
                    self.exchange = None;
                    return Ok(State::Complete(Ok(credentials)));
                }
            }
        }
    }

    /// Our long-term public key, signed, and our name, encrypted.
    fn m5(&self, key: &[u8]) -> Result<Vec<u8>, Error> {
        let controller_x = derive(
            key,
            b"Pair-Setup-Controller-Sign-Salt",
            b"Pair-Setup-Controller-Sign-Info",
        );
        let id = self.keypair.id.as_bytes();
        let public_key = self.keypair.ltpk();
        let signature = self
            .keypair
            .sign(&[&controller_x[..], id, &public_key].concat());
        let name = pack(&Value::Dict(vec![(
            "name".into(),
            self.name.as_str().into(),
        )]))?;

        let mut inner = Vec::new();
        let mut tlv = Tlv8Writer::new(&mut inner);
        tlv.push(IDENTIFIER, id);
        tlv.push(PUBLIC_KEY, &public_key);
        tlv.push(SIGNATURE, &signature);
        tlv.push(NAME, &name);
        let encrypted =
            chacha20poly1305_seal(&encryption_key(key), &label_nonce(b"PS-Msg05"), b"", &inner)?;

        let mut m5 = Vec::new();
        let mut tlv = Tlv8Writer::new(&mut m5);
        tlv.push_u8(STATE, 5);
        tlv.push(ENCRYPTED_DATA, &encrypted);
        Ok(m5)
    }

    /// The device's long-term public key, checked against its signature.
    fn handle_m6(&self, key: &[u8], m6: &Tlv8Map) -> Result<Credentials, Error> {
        let encrypted = m6
            .get(ENCRYPTED_DATA)
            .ok_or(Error::Malformed("pair setup M6 without encrypted data"))?;
        let inner = chacha20poly1305_open(
            &encryption_key(key),
            &label_nonce(b"PS-Msg06"),
            b"",
            encrypted,
        )?;
        let inner = Tlv8Map::parse(&inner)?;
        let id = inner
            .get(IDENTIFIER)
            .ok_or(Error::Malformed("pair setup M6 without an identifier"))?;
        let public_key: [u8; 32] = inner
            .get(PUBLIC_KEY)
            .and_then(|key| key.try_into().ok())
            .ok_or(Error::Malformed(
                "pair setup M6 without a 32-byte public key",
            ))?;
        let signature: [u8; 64] = inner
            .get(SIGNATURE)
            .and_then(|signature| signature.try_into().ok())
            .ok_or(Error::Malformed(
                "pair setup M6 without a 64-byte signature",
            ))?;
        let device_x = derive(
            key,
            b"Pair-Setup-Accessory-Sign-Salt",
            b"Pair-Setup-Accessory-Sign-Info",
        );
        verify_ed25519(
            &public_key,
            &[&device_x[..], id, &public_key].concat(),
            &signature,
        )?;
        Ok(Credentials {
            device_id: String::from_utf8(id.to_vec())
                .map_err(|_| Error::Malformed("pair setup M6 identifier is not UTF-8"))?,
            device_public_key: public_key,
            client_id: self.keypair.id.clone(),
            client_secret_key: self.keypair.seed(),
        })
    }
}

impl Coroutine for PairSetupFinish {
    type Output = Credentials;

    fn resume(&mut self, arg: Option<Reply<'_>>) -> State<Credentials> {
        self.step(arg)
            .unwrap_or_else(|error| State::Complete(Err(error)))
    }
}

/// Our side of SRP-6a, with the 3072-bit group and SHA-512.
///
/// The `srp` crate does the modular arithmetic. Its RFC 5054 proof hashes
/// `PAD(g)` where HomeKit hashes `g`, so the hashes are done here, every
/// number padded to the length of N as HomeKit does.
struct Srp {
    a_pub: Vec<u8>,
    m1: Vec<u8>,
    m2: Vec<u8>,
    key: Vec<u8>,
}

impl Srp {
    fn new(pin: &str, m2: &[u8], a: &[u8; 32]) -> Result<Self, Error> {
        let m2 = Tlv8Map::parse(m2)?;
        let salt = m2
            .get(SALT)
            .ok_or(Error::Malformed("pair setup M2 without a salt"))?;
        let b_pub = m2
            .get(PUBLIC_KEY)
            .ok_or(Error::Malformed("pair setup M2 without a public key"))?;

        let g = G3072::generator();
        let n = g.params().modulus().to_be_bytes().to_vec();
        let padded_g = pad(&g.retrieve().to_be_bytes(), n.len()).expect("g is shorter than N");
        let b_pub =
            pad(b_pub, n.len()).ok_or(Error::Malformed("pair setup M2 public key is too long"))?;
        // B must be a nonzero number below N: B = 0 mod N would make the
        // session key predictable.
        if b_pub.iter().all(|byte| *byte == 0) || b_pub >= n {
            return Err(Error::Malformed("pair setup M2 public key is out of range"));
        }

        let client = srp::ClientG3072::<Sha512>::new();
        let a_pub =
            pad(&client.compute_public_ephemeral(a), n.len()).expect("g^a mod N is shorter than N");
        let k = number(&hash(&[&n, &padded_g]));
        let u = number(&hash(&[&a_pub, &b_pub]));
        let identity = srp::ClientG3072::<Sha512>::compute_identity_hash(USERNAME, pin.as_bytes());
        let x = srp::ClientG3072::<Sha512>::compute_x(&identity, salt);
        let premaster = client.compute_premaster_secret(&number(&b_pub), &k, &x, &number(a), &u);
        let premaster = pad(&premaster.to_be_bytes_trimmed_vartime(), n.len())
            .expect("S mod N is shorter than N");
        let key = hash(&[&premaster]);

        let h_n = hash(&[&n]);
        let h_g = hash(&[trim(&padded_g)]);
        let h_n_xor_h_g: Vec<u8> = h_n.iter().zip(&h_g).map(|(a, b)| a ^ b).collect();
        let m1 = hash(&[&h_n_xor_h_g, &hash(&[USERNAME]), salt, &a_pub, &b_pub, &key]);
        let m2 = hash(&[&a_pub, &m1, &key]);
        Ok(Srp { a_pub, m1, m2, key })
    }
}

fn hash(parts: &[&[u8]]) -> Vec<u8> {
    let mut hasher = Sha512::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().to_vec()
}

fn number(bytes: &[u8]) -> BoxedUint {
    BoxedUint::from_be_slice_vartime(bytes)
}

/// `bytes` with leading zeros added to make it `length` long.
fn pad(bytes: &[u8], length: usize) -> Option<Vec<u8>> {
    let bytes = trim(bytes);
    let mut padded = vec![0; length.checked_sub(bytes.len())?];
    padded.extend_from_slice(bytes);
    Some(padded)
}

fn trim(bytes: &[u8]) -> &[u8] {
    let zeros = bytes.iter().take_while(|byte| **byte == 0).count();
    &bytes[zeros..]
}

fn encryption_key(key: &[u8]) -> [u8; 32] {
    derive(key, b"Pair-Setup-Encrypt-Salt", b"Pair-Setup-Encrypt-Info")
}

/// The fields of a pair setup message carrying `tlv`, with a PIN as the
/// password type.
fn fields(tlv: Vec<u8>) -> Vec<(Value, Value)> {
    vec![field(tlv), ("_pwTy".into(), Value::int(1))]
}
