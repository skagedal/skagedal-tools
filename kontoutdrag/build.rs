//! Build script for kontoutdrag.
//!
//! With the `web` feature on, builds the React app under `browser/` so the
//! `include_dir!` in `src/web.rs` embeds an up-to-date copy. Without it,
//! does nothing.

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_WEB");
    if env::var("CARGO_FEATURE_WEB").is_err() {
        return;
    }
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let browser = manifest.join("browser");
    webview_shell::build::build_web_app(
        &browser,
        "build",
        &browser.join("dist/index.html"),
        &[&browser.join("src"), &browser.join("index.html")],
    );
}
