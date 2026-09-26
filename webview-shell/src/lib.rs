//! The plumbing shared by the tools that show a React app in a webview:
//! build it with pnpm and Vite from `build.rs`, embed it with `include_dir!`,
//! serve it from a localhost HTTP server beside a small JSON API, and open a
//! wry window on that URL.
//!
//! `build` has no dependencies, so a build script can use it. `server` and
//! `window` are behind the `app` feature.

pub mod build;
#[cfg(feature = "app")]
pub mod server;
#[cfg(feature = "app")]
pub mod window;
