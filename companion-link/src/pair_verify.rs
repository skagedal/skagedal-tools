//! Pair verify: prove both sides still hold the keys from pairing, and set up
//! session encryption.

use apple_opack::Value;
use hap_crypto::aead::{chacha20poly1305_open, chacha20poly1305_seal};
use hap_crypto::{ControllerKeypair, EphemeralKeypair, verify_ed25519};
use hap_tlv8::{Tlv8Map, Tlv8Writer};

use crate::coroutine::{Coroutine, Reply, State};
use crate::credentials::Credentials;
use crate::error::Error;
use crate::exchange::Exchange;
use crate::frame::FrameType;
use crate::pairing_data::{
    ENCRYPTED_DATA, IDENTIFIER, PUBLIC_KEY, SIGNATURE, STATE, derive, field, label_nonce,
    pairing_data,
};
use crate::session::Session;

/// Authentication type: pair verify with keys from pair setup.
const AUTH_TYPE: u64 = 4;

#[derive(Debug)]
enum Phase {
    /// M1 sent, waiting for M2.
    AwaitM2,
    /// M3 sent, waiting for M4; the shared secret is kept for the session
    /// keys.
    AwaitM4([u8; 32]),
}

/// Pair verify, completing with the session encrypted.
pub struct PairVerify {
    session: Option<Session>,
    credentials: Credentials,
    ephemeral: EphemeralKeypair,
    exchange: Option<Exchange>,
    phase: Phase,
    error: Option<Error>,
}

impl PairVerify {
    /// Verify `credentials` over `session`, using `ephemeral_secret`, which
    /// should be fresh and random, for the key exchange.
    pub fn new(mut session: Session, credentials: Credentials, ephemeral_secret: [u8; 32]) -> Self {
        let ephemeral = EphemeralKeypair::from_secret(ephemeral_secret);
        let mut m1 = Vec::new();
        let mut tlv = Tlv8Writer::new(&mut m1);
        tlv.push_u8(STATE, 1);
        tlv.push(PUBLIC_KEY, &ephemeral.public());
        let fields = vec![field(m1), ("_auTy".into(), Value::int(AUTH_TYPE))];
        let (exchange, error) = match Exchange::auth(&mut session, FrameType::PV_START, fields) {
            Ok(exchange) => (Some(exchange), None),
            Err(error) => (None, Some(error)),
        };
        PairVerify {
            session: Some(session),
            credentials,
            ephemeral,
            exchange,
            phase: Phase::AwaitM2,
            error,
        }
    }

    fn step(&mut self, mut arg: Option<Reply<'_>>) -> Result<State<Session>, Error> {
        if let Some(error) = self.error.take() {
            return Err(error);
        }
        loop {
            let session = self.session.as_mut().ok_or(Error::Finished)?;
            let exchange = self.exchange.as_mut().ok_or(Error::Finished)?;
            let message = match exchange.resume(session, arg.take()) {
                State::Complete(result) => result?,
                State::Yielded(wants) => return Ok(State::Yielded(wants)),
            };
            let tlv = pairing_data(&message)?;
            match self.phase {
                Phase::AwaitM2 => {
                    let (m3, shared) = self.handle_m2(&tlv)?;
                    let session = self.session.as_mut().ok_or(Error::Finished)?;
                    self.exchange = Some(Exchange::auth(
                        session,
                        FrameType::PV_NEXT,
                        vec![field(m3)],
                    )?);
                    self.phase = Phase::AwaitM4(shared);
                }
                Phase::AwaitM4(shared) => {
                    self.exchange = None;
                    let mut session = self.session.take().ok_or(Error::Finished)?;
                    let out_key = derive(&shared, b"", b"ClientEncrypt-main");
                    let in_key = derive(&shared, b"", b"ServerEncrypt-main");
                    session.enable_encryption(out_key, in_key);
                    return Ok(State::Complete(Ok(session)));
                }
            }
        }
    }

    /// Check the device's proof in M2, and answer with ours in M3.
    fn handle_m2(&self, tlv: &[u8]) -> Result<(Vec<u8>, [u8; 32]), Error> {
        let m2 = Tlv8Map::parse(tlv)?;
        let device_public: [u8; 32] = m2
            .get(PUBLIC_KEY)
            .and_then(|key| key.try_into().ok())
            .ok_or(Error::Malformed(
                "pair verify M2 without a 32-byte public key",
            ))?;
        let encrypted = m2
            .get(ENCRYPTED_DATA)
            .ok_or(Error::Malformed("pair verify M2 without encrypted data"))?;

        let our_public = self.ephemeral.public();
        let shared = self.ephemeral.diffie_hellman(&device_public);
        let key = derive(
            &shared,
            b"Pair-Verify-Encrypt-Salt",
            b"Pair-Verify-Encrypt-Info",
        );

        let proof = chacha20poly1305_open(&key, &label_nonce(b"PV-Msg02"), b"", encrypted)?;
        let proof = Tlv8Map::parse(&proof)?;
        let identifier = proof
            .get(IDENTIFIER)
            .ok_or(Error::Malformed("pair verify M2 without an identifier"))?;
        let signature: [u8; 64] = proof
            .get(SIGNATURE)
            .and_then(|signature| signature.try_into().ok())
            .ok_or(Error::Malformed(
                "pair verify M2 without a 64-byte signature",
            ))?;
        if identifier != self.credentials.device_id.as_bytes() {
            return Err(Error::Malformed(
                "pair verify M2 is from a different device",
            ));
        }
        verify_ed25519(
            &self.credentials.device_public_key,
            &[&device_public[..], identifier, &our_public].concat(),
            &signature,
        )?;

        let keypair = ControllerKeypair::from_seed(
            self.credentials.client_id.clone(),
            self.credentials.client_secret_key,
        );
        let client_id = self.credentials.client_id.as_bytes();
        let signature = keypair.sign(&[&our_public[..], client_id, &device_public].concat());
        let mut proof = Vec::new();
        let mut tlv = Tlv8Writer::new(&mut proof);
        tlv.push(IDENTIFIER, client_id);
        tlv.push(SIGNATURE, &signature);
        let encrypted = chacha20poly1305_seal(&key, &label_nonce(b"PV-Msg03"), b"", &proof)?;

        let mut m3 = Vec::new();
        let mut tlv = Tlv8Writer::new(&mut m3);
        tlv.push_u8(STATE, 3);
        tlv.push(ENCRYPTED_DATA, &encrypted);
        Ok((m3, shared))
    }
}

impl Coroutine for PairVerify {
    type Output = Session;

    fn resume(&mut self, arg: Option<Reply<'_>>) -> State<Session> {
        self.step(arg)
            .unwrap_or_else(|error| State::Complete(Err(error)))
    }
}
