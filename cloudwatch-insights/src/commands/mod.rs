use std::fmt;

pub mod copy_link;
pub mod edit_config;
pub mod paste_link;
pub mod query;
pub mod raw;
pub mod show;

/// Error variant carrying both an exit code and a user-visible message.
/// Returned via `anyhow::Error` so subcommand return types stay simple.
#[derive(Debug)]
pub struct ExitError {
    pub code: u8,
    pub message: String,
}

impl fmt::Display for ExitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ExitError {}

/// Build an `anyhow::Error` carrying an `ExitError`.
pub fn fail(code: u8, message: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(ExitError {
        code,
        message: message.into(),
    })
}
