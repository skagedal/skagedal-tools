//! Requests over an encrypted session.

use apple_opack::Value;

use crate::coroutine::{Coroutine, Reply, State};
use crate::error::Error;
use crate::exchange::Exchange;
use crate::session::Session;

/// Send one request and wait for its response, completing with the session
/// and the response content (`_c`), which is [`Value::Null`] when absent.
pub struct Request {
    session: Option<Session>,
    exchange: Result<Exchange, Option<Error>>,
}

impl Request {
    pub fn new(mut session: Session, identifier: &str, content: Value) -> Self {
        let exchange = Exchange::request(&mut session, identifier, content);
        Request {
            session: Some(session),
            exchange: exchange.map_err(Some),
        }
    }

    fn step(&mut self, arg: Option<Reply<'_>>) -> Result<State<(Session, Value)>, Error> {
        let exchange = self
            .exchange
            .as_mut()
            .map_err(|error| error.take().unwrap_or(Error::Finished))?;
        let session = self.session.as_mut().ok_or(Error::Finished)?;
        let message = match exchange.resume(session, arg) {
            State::Complete(result) => result?,
            State::Yielded(wants) => return Ok(State::Yielded(wants)),
        };
        let session = self.session.take().ok_or(Error::Finished)?;
        let content = message.get("_c").cloned().unwrap_or(Value::Null);
        Ok(State::Complete(Ok((session, content))))
    }
}

impl Coroutine for Request {
    type Output = (Session, Value);

    fn resume(&mut self, arg: Option<Reply<'_>>) -> State<(Session, Value)> {
        self.step(arg)
            .unwrap_or_else(|error| State::Complete(Err(error)))
    }
}
