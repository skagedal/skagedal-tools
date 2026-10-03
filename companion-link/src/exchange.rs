//! Send one message and wait for its answer: the step every coroutine in
//! this crate is built from.

use apple_opack::Value;
use log::trace;

use crate::coroutine::{Reply, State, Wants};
use crate::error::Error;
use crate::frame::FrameType;
use crate::session::Session;

const MESSAGE_TYPE_REQUEST: u64 = 2;
const MESSAGE_TYPE_RESPONSE: u64 = 3;

#[derive(Debug)]
enum Expect {
    /// Pairing frames have no transaction id; the answer to `*_START` comes
    /// as `*_NEXT`, like every later one.
    Auth(FrameType),
    Response {
        identifier: String,
        xid: u64,
    },
}

#[derive(Debug)]
pub(crate) struct Exchange {
    request: Option<Vec<u8>>,
    expect: Expect,
}

impl Exchange {
    /// A pairing message: `fields` sent in a frame of `frame_type`.
    pub(crate) fn auth(
        session: &mut Session,
        frame_type: FrameType,
        mut fields: Vec<(Value, Value)>,
    ) -> Result<Self, Error> {
        let answer = match frame_type {
            FrameType::PS_START => FrameType::PS_NEXT,
            FrameType::PV_START => FrameType::PV_NEXT,
            other => other,
        };
        fields.push(("_x".into(), Value::int(session.take_xid())));
        let request = session.encode(frame_type, &Value::Dict(fields))?;
        Ok(Exchange {
            request: Some(request),
            expect: Expect::Auth(answer),
        })
    }

    /// A request `identifier` with `content`.
    pub(crate) fn request(
        session: &mut Session,
        identifier: &str,
        content: Value,
    ) -> Result<Self, Error> {
        let xid = session.take_xid();
        let message = Value::Dict(vec![
            ("_i".into(), identifier.into()),
            ("_t".into(), Value::int(MESSAGE_TYPE_REQUEST)),
            ("_c".into(), content),
            ("_x".into(), Value::int(xid)),
        ]);
        let request = session.encode(FrameType::E_OPACK, &message)?;
        Ok(Exchange {
            request: Some(request),
            expect: Expect::Response {
                identifier: identifier.to_string(),
                xid,
            },
        })
    }

    /// Resume, completing with the whole answering message.
    pub(crate) fn resume(&mut self, session: &mut Session, arg: Option<Reply<'_>>) -> State<Value> {
        match self.step(session, arg) {
            Ok(Some(message)) => State::Complete(Ok(message)),
            Ok(None) => State::Yielded(self.request.take().map_or(Wants::Read, Wants::Write)),
            Err(error) => State::Complete(Err(error)),
        }
    }

    fn step(
        &mut self,
        session: &mut Session,
        arg: Option<Reply<'_>>,
    ) -> Result<Option<Value>, Error> {
        if self.request.is_some() {
            return Ok(None);
        }
        match arg {
            Some(Reply::Read([])) => return Err(Error::Closed),
            Some(Reply::Read(bytes)) => session.feed(bytes),
            Some(Reply::Wrote) | None => {}
        }
        while let Some((frame_type, message)) = session.next_message()? {
            if self.answers(frame_type, &message) {
                return self.check(message).map(Some);
            }
            trace!("ignore unrelated {frame_type:?} message");
        }
        Ok(None)
    }

    fn answers(&self, frame_type: FrameType, message: &Value) -> bool {
        match &self.expect {
            Expect::Auth(expected) => frame_type == *expected,
            Expect::Response { xid, .. } => {
                frame_type == FrameType::E_OPACK
                    && message.get("_t").and_then(Value::as_u64) == Some(MESSAGE_TYPE_RESPONSE)
                    && message.get("_x").and_then(Value::as_u64) == Some(*xid)
            }
        }
    }

    fn check(&self, message: Value) -> Result<Value, Error> {
        let Some(error) = message.get("_em") else {
            return Ok(message);
        };
        let identifier = match &self.expect {
            Expect::Auth(_) => "pairing".to_string(),
            Expect::Response { identifier, .. } => identifier.clone(),
        };
        Err(Error::Request {
            identifier,
            message: error.as_str().unwrap_or("unknown error").to_string(),
        })
    }
}
