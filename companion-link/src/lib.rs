//! The parts of Apple's Companion link protocol needed to pair with an Apple
//! TV and send it requests, as IO-free coroutines.
//!
//! A coroutine is resumed with what the caller did last, and answers with
//! what it wants done next: bytes written to the connection, or more bytes
//! read from it. It never touches a socket, a clock or a source of
//! randomness; the caller passes those in. The `client` feature adds a
//! blocking driver over `std::net::TcpStream`.

pub mod cipher;
#[cfg(feature = "client")]
pub mod client;
pub mod coroutine;
pub mod credentials;
pub mod error;
mod exchange;
pub mod frame;
pub mod pair_setup;
pub mod pair_verify;
mod pairing_data;
pub mod request;
pub mod session;
