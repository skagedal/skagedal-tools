//! The contract every coroutine in this crate follows.

/// What a coroutine needs done before it can continue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Wants {
    /// Write all of these bytes to the connection.
    Write(Vec<u8>),
    /// Read whatever is available from the connection.
    Read,
}

/// What the caller did since the last resume.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reply<'a> {
    /// The bytes asked for by [`Wants::Write`] were written.
    Wrote,
    /// These bytes were read; empty means the connection was closed.
    Read(&'a [u8]),
}

/// Where a coroutine stands after a resume.
#[derive(Debug)]
pub enum State<T> {
    /// It needs the caller to do something, then resume it.
    Yielded(Wants),
    /// It is done.
    Complete(Result<T, crate::error::Error>),
}

/// A state machine driven by the caller's IO.
pub trait Coroutine {
    /// What the coroutine produces when it completes.
    type Output;

    /// Continue, given what was done since the last call: `None` the first
    /// time, then a [`Reply`] to each [`Wants`].
    fn resume(&mut self, arg: Option<Reply<'_>>) -> State<Self::Output>;
}
