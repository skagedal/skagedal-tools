//! Pair verify and requests against a fake Apple TV, run entirely in memory.

use apple_opack::{Value, pack, unpack};
use companion_link::cipher::{Cipher, TAG_LENGTH};
use companion_link::coroutine::{Coroutine, Reply, State, Wants};
use companion_link::credentials::Credentials;
use companion_link::error::{Error, Refusal};
use companion_link::frame::{FrameBuffer, FrameType, header};
use companion_link::pair_verify::PairVerify;
use companion_link::request::Request;
use companion_link::session::Session;
use hap_crypto::aead::{chacha20poly1305_open, chacha20poly1305_seal};
use hap_crypto::{ControllerKeypair, EphemeralKeypair, verify_ed25519};
use hap_tlv8::{Tlv8Map, Tlv8Writer};
use hkdf::Hkdf;
use sha2::Sha512;

const DEVICE_ID: &str = "E3E25DF5-AB48-40F0-B61D-2AADADAB0FBD";
const CLIENT_ID: &str = "01c71d5d-14b3-4486-877a-60d9304510d6";

/// The device's side of the conversation.
struct FakeDevice {
    keypair: ControllerKeypair,
    ephemeral: EphemeralKeypair,
    client_public_key: [u8; 32],
    incoming: FrameBuffer,
    cipher: Option<Cipher>,
    shared: Option<[u8; 32]>,
    client_ephemeral: Option<[u8; 32]>,
    /// Answers requests with this error instead of success.
    fail_with: Option<&'static str>,
    /// Refuses pair verify with this TLV8 error code.
    refuse_with: Option<u8>,
    requests: Vec<Value>,
}

impl FakeDevice {
    fn new() -> (Self, Credentials) {
        let keypair = ControllerKeypair::from_seed(DEVICE_ID.into(), [9; 32]);
        let client = ControllerKeypair::from_seed(CLIENT_ID.into(), [3; 32]);
        let credentials = Credentials {
            device_id: DEVICE_ID.into(),
            device_public_key: keypair.ltpk(),
            client_id: CLIENT_ID.into(),
            client_secret_key: client.seed(),
        };
        let device = FakeDevice {
            keypair,
            ephemeral: EphemeralKeypair::from_secret([7; 32]),
            client_public_key: client.ltpk(),
            incoming: FrameBuffer::default(),
            cipher: None,
            shared: None,
            client_ephemeral: None,
            fail_with: None,
            refuse_with: None,
            requests: vec![],
        };
        (device, credentials)
    }

    /// Take bytes the client wrote, and return what the device answers.
    fn receive(&mut self, bytes: &[u8]) -> Vec<u8> {
        self.incoming.extend(bytes);
        let mut out = vec![];
        while let Some((header, payload)) = self.incoming.pop() {
            let payload = match &mut self.cipher {
                Some(cipher) => cipher.decrypt(&header, &payload).unwrap(),
                None => payload,
            };
            let (message, _) = unpack(&payload).unwrap();
            out.extend(self.answer(FrameType(header[0]), &message));
        }
        out
    }

    fn answer(&mut self, frame_type: FrameType, message: &Value) -> Vec<u8> {
        match frame_type {
            FrameType::PV_START => self.m2(message),
            FrameType::PV_NEXT => self.m4(message),
            FrameType::E_OPACK => self.response(message),
            other => panic!("unexpected frame {other:?}"),
        }
    }

    fn m2(&mut self, message: &Value) -> Vec<u8> {
        assert_eq!(message.get("_auTy").and_then(Value::as_u64), Some(4));
        let m1 = Tlv8Map::parse(message.get("_pd").unwrap().as_bytes().unwrap()).unwrap();
        if let Some(code) = self.refuse_with {
            let mut tlv = vec![];
            let mut writer = Tlv8Writer::new(&mut tlv);
            writer.push_u8(0x06, 2);
            writer.push_u8(0x07, code);
            return self.frame(FrameType::PV_NEXT, pairing(tlv));
        }
        let client_ephemeral: [u8; 32] = m1.get(0x03).unwrap().try_into().unwrap();
        let shared = self.ephemeral.diffie_hellman(&client_ephemeral);
        let ours = self.ephemeral.public();
        let signature = self
            .keypair
            .sign(&[&ours[..], DEVICE_ID.as_bytes(), &client_ephemeral].concat());
        let mut proof = vec![];
        let mut writer = Tlv8Writer::new(&mut proof);
        writer.push(0x01, DEVICE_ID.as_bytes());
        writer.push(0x0A, &signature);
        let key = derive(
            &shared,
            b"Pair-Verify-Encrypt-Salt",
            b"Pair-Verify-Encrypt-Info",
        );
        let encrypted = chacha20poly1305_seal(&key, &nonce(b"PV-Msg02"), b"", &proof).unwrap();

        let mut tlv = vec![];
        let mut writer = Tlv8Writer::new(&mut tlv);
        writer.push_u8(0x06, 2);
        writer.push(0x03, &ours);
        writer.push(0x05, &encrypted);
        self.shared = Some(shared);
        self.client_ephemeral = Some(client_ephemeral);
        self.frame(FrameType::PV_NEXT, pairing(tlv))
    }

    fn m4(&mut self, message: &Value) -> Vec<u8> {
        let shared = self.shared.unwrap();
        let m3 = Tlv8Map::parse(message.get("_pd").unwrap().as_bytes().unwrap()).unwrap();
        let key = derive(
            &shared,
            b"Pair-Verify-Encrypt-Salt",
            b"Pair-Verify-Encrypt-Info",
        );
        let proof =
            chacha20poly1305_open(&key, &nonce(b"PV-Msg03"), b"", m3.get(0x05).unwrap()).unwrap();
        let proof = Tlv8Map::parse(&proof).unwrap();
        assert_eq!(proof.get(0x01).unwrap(), CLIENT_ID.as_bytes());
        let signature: [u8; 64] = proof.get(0x0A).unwrap().try_into().unwrap();
        let signed = [
            &self.client_ephemeral.unwrap()[..],
            CLIENT_ID.as_bytes(),
            &self.ephemeral.public(),
        ]
        .concat();
        verify_ed25519(&self.client_public_key, &signed, &signature).unwrap();

        let mut tlv = vec![];
        Tlv8Writer::new(&mut tlv).push_u8(0x06, 4);
        let m4 = self.frame(FrameType::PV_NEXT, pairing(tlv));
        self.cipher = Some(Cipher::new(
            derive(&shared, b"", b"ServerEncrypt-main"),
            derive(&shared, b"", b"ClientEncrypt-main"),
        ));
        m4
    }

    fn response(&mut self, message: &Value) -> Vec<u8> {
        self.requests.push(message.clone());
        let xid = message.get("_x").and_then(Value::as_u64).unwrap();
        // An event the client did not ask for comes first, as real devices
        // send them whenever they like.
        let mut out = self.frame(
            FrameType::E_OPACK,
            Value::Dict(vec![
                ("_i".into(), "_iMC".into()),
                ("_t".into(), Value::int(1)),
                ("_c".into(), Value::Dict(vec![])),
            ]),
        );
        let mut response = vec![("_t".into(), Value::int(3)), ("_x".into(), Value::int(xid))];
        match self.fail_with {
            Some(error) => response.push(("_em".into(), error.into())),
            None => response.push(("_c".into(), Value::Dict(vec![("ok".into(), true.into())]))),
        }
        out.extend(self.frame(FrameType::E_OPACK, Value::Dict(response)));
        out
    }

    fn frame(&mut self, frame_type: FrameType, message: Value) -> Vec<u8> {
        let payload = pack(&message).unwrap();
        match &mut self.cipher {
            Some(cipher) => {
                let header = header(frame_type, payload.len() + TAG_LENGTH);
                [&header[..], &cipher.encrypt(&header, &payload).unwrap()].concat()
            }
            None => [&header(frame_type, payload.len())[..], &payload].concat(),
        }
    }
}

/// Run `coroutine` against `device`, handing over what the device writes a
/// few bytes at a time to exercise reassembly of frames.
fn run<C: Coroutine>(device: &mut FakeDevice, mut coroutine: C) -> Result<C::Output, Error> {
    let mut pending: Vec<u8> = vec![];
    let mut chunk: Vec<u8> = vec![];
    let mut arg = None;
    loop {
        let state = coroutine.resume(arg.take().map(|wrote: bool| {
            if wrote {
                Reply::Wrote
            } else {
                Reply::Read(&chunk)
            }
        }));
        match state {
            State::Yielded(Wants::Write(bytes)) => {
                pending.extend(device.receive(&bytes));
                arg = Some(true);
            }
            State::Yielded(Wants::Read) => {
                let take = pending.len().min(5);
                chunk = pending.drain(..take).collect();
                arg = Some(false);
            }
            State::Complete(result) => return result,
        }
    }
}

fn verified(device: &mut FakeDevice, credentials: Credentials) -> Session {
    run(
        device,
        PairVerify::new(Session::new(100), credentials, [5; 32]),
    )
    .unwrap()
}

#[test]
fn pair_verify_encrypts_the_session() {
    let (mut device, credentials) = FakeDevice::new();
    let session = verified(&mut device, credentials);
    assert!(session.is_encrypted());
}

#[test]
fn request_gets_its_response_past_an_event() {
    let (mut device, credentials) = FakeDevice::new();
    let session = verified(&mut device, credentials);
    let content = Value::Dict(vec![("_urlS".into(), "vlc://host/movie.mkv".into())]);
    let (session, response) =
        run(&mut device, Request::new(session, "_launchApp", content)).unwrap();
    assert_eq!(response.get("ok"), Some(&Value::Bool(true)));

    let request = &device.requests[0];
    assert_eq!(
        request.get("_i").and_then(Value::as_str),
        Some("_launchApp")
    );
    assert_eq!(request.get("_t").and_then(Value::as_u64), Some(2));
    let url = request.get("_c").and_then(|content| content.get("_urlS"));
    assert_eq!(url.and_then(Value::as_str), Some("vlc://host/movie.mkv"));

    // The session keeps working, counters and all.
    run(
        &mut device,
        Request::new(session, "_launchApp", Value::Dict(vec![])),
    )
    .unwrap();
}

#[test]
fn request_error_is_reported() {
    let (mut device, credentials) = FakeDevice::new();
    let session = verified(&mut device, credentials);
    device.fail_with = Some("No such app");
    let error = run(
        &mut device,
        Request::new(session, "_launchApp", Value::Dict(vec![])),
    )
    .unwrap_err();
    assert!(
        matches!(error, Error::Request { ref identifier, ref message }
        if identifier == "_launchApp" && message == "No such app")
    );
}

#[test]
fn pair_verify_rejects_another_device() {
    let (mut device, mut credentials) = FakeDevice::new();
    credentials.device_public_key = ControllerKeypair::from_seed("other".into(), [1; 32]).ltpk();
    let result = run(
        &mut device,
        PairVerify::new(Session::new(1), credentials, [5; 32]),
    );
    assert!(matches!(result, Err(Error::Crypto(_))));
}

#[test]
fn pair_verify_reports_refusal() {
    let (mut device, credentials) = FakeDevice::new();
    device.refuse_with = Some(0x02);
    let result = run(
        &mut device,
        PairVerify::new(Session::new(1), credentials, [5; 32]),
    );
    assert!(matches!(
        result,
        Err(Error::Refused(Refusal::Authentication))
    ));
}

#[test]
fn closed_connection_is_an_error() {
    let (_, credentials) = FakeDevice::new();
    let mut verify = PairVerify::new(Session::new(1), credentials, [5; 32]);
    assert!(matches!(
        verify.resume(None),
        State::Yielded(Wants::Write(_))
    ));
    assert!(matches!(
        verify.resume(Some(Reply::Wrote)),
        State::Yielded(Wants::Read)
    ));
    assert!(matches!(
        verify.resume(Some(Reply::Read(&[]))),
        State::Complete(Err(Error::Closed))
    ));
}

fn pairing(tlv: Vec<u8>) -> Value {
    Value::Dict(vec![("_pd".into(), Value::Bytes(tlv))])
}

fn derive(shared: &[u8; 32], salt: &[u8], info: &[u8]) -> [u8; 32] {
    let mut key = [0; 32];
    Hkdf::<Sha512>::new(Some(salt), shared)
        .expand(info, &mut key)
        .unwrap();
    key
}

fn nonce(label: &[u8; 8]) -> [u8; 12] {
    let mut nonce = [0; 12];
    nonce[4..].copy_from_slice(label);
    nonce
}
