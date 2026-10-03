//! Pair setup: trade a PIN shown on the device for long-term keys.
//!
//! Split in two where the device shows its PIN: [`PairSetupStart`] asks for
//! it, and [`PairSetupFinish`] continues with the PIN the user typed.

use apple_opack::Value;
use hap_crypto::{ControllerKeypair, PairSetupClient, PairSetupStep};
use hap_tlv8::Tlv8Writer;

use crate::coroutine::{Coroutine, Reply, State};
use crate::credentials::Credentials;
use crate::error::Error;
use crate::exchange::Exchange;
use crate::frame::FrameType;
use crate::pairing_data::{METHOD, STATE, field, pairing_data};
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

/// Pair setup from the PIN on.
pub struct PairSetupFinish {
    session: Session,
    keypair: ControllerKeypair,
    client: Option<PairSetupClient>,
    exchange: Option<Exchange>,
    error: Option<Error>,
}

impl PairSetupFinish {
    /// Continue `pending` with the `pin` shown on the device.
    ///
    /// `client_id` and `client_secret_key` become our identity with this
    /// device, and `srp_private` is the secret SRP exponent; all three should
    /// be fresh and random.
    pub fn new(
        pending: PairSetupPending,
        pin: &str,
        client_id: String,
        client_secret_key: [u8; 32],
        srp_private: [u8; 32],
    ) -> Self {
        let mut this = PairSetupFinish {
            session: pending.session,
            keypair: ControllerKeypair::from_seed(client_id, client_secret_key),
            client: None,
            exchange: None,
            error: None,
        };
        if let Err(error) = this.begin(pin, &srp_private, &pending.m2) {
            this.error = Some(error);
        }
        this
    }

    fn begin(&mut self, pin: &str, srp_private: &[u8], m2: &[u8]) -> Result<(), Error> {
        let mut client = PairSetupClient::new_with_private(pin, self.keypair.clone(), srp_private)?;
        // The device already has M1, from PairSetupStart; this only puts the
        // client in the state of waiting for M2.
        let _ = client.start();
        let step = client.handle(m2)?;
        self.client = Some(client);
        match self.advance(step)? {
            None => Ok(()),
            Some(_) => Err(Error::Malformed("pair setup finished before M3")),
        }
    }

    /// Act on what the client wants next: send it, or finish.
    fn advance(&mut self, step: PairSetupStep) -> Result<Option<Credentials>, Error> {
        match step {
            PairSetupStep::Send(next) => {
                self.exchange = Some(Exchange::auth(
                    &mut self.session,
                    FrameType::PS_NEXT,
                    fields(next),
                )?);
                Ok(None)
            }
            PairSetupStep::Done(device) => {
                self.exchange = None;
                Ok(Some(Credentials {
                    device_id: device.pairing_id,
                    device_public_key: device.ltpk,
                    client_id: self.keypair.id.clone(),
                    client_secret_key: self.keypair.seed(),
                }))
            }
        }
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
            let client = self.client.as_mut().ok_or(Error::Finished)?;
            let step = client.handle(&pairing_data(&message)?)?;
            if let Some(credentials) = self.advance(step)? {
                return Ok(State::Complete(Ok(credentials)));
            }
        }
    }
}

impl Coroutine for PairSetupFinish {
    type Output = Credentials;

    fn resume(&mut self, arg: Option<Reply<'_>>) -> State<Credentials> {
        self.step(arg)
            .unwrap_or_else(|error| State::Complete(Err(error)))
    }
}

/// The fields of a pair setup message carrying `tlv`, with a PIN as the
/// password type.
fn fields(tlv: Vec<u8>) -> Vec<(Value, Value)> {
    vec![field(tlv), ("_pwTy".into(), Value::int(1))]
}
