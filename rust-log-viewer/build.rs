use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_WEB");
    if env::var("CARGO_FEATURE_WEB").is_err() {
        return;
    }
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let dist = manifest.join("../log-viewer/web/dist");
    let index = dist.join("index.html");
    // include_dir! captures whatever's at the path at build time, so re-run if
    // the React build output changes underneath us.
    println!("cargo:rerun-if-changed={}", dist.display());
    if !index.exists() {
        eprintln!(
            "\nerror: rust-log-viewer was built with the `web` feature, but the React app dist is missing.\n\
             Looked for: {}\n\n\
             Build it first with one of:\n  \
             pnpm --dir log-viewer install && pnpm --dir log-viewer run build:web\n  \
             ./install rust-log-viewer\n",
            index.display()
        );
        std::process::exit(1);
    }
}
