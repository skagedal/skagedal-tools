//! Build script for log-viewer.
//!
//! When the `web` feature is enabled, this builds the React app under
//! `browser/` so `include_dir!("$CARGO_MANIFEST_DIR/browser/web/dist")`
//! in `src/web/server.rs` picks up an up-to-date copy. Without the `web`
//! feature this is a no-op.
//!
//! The build runs `pnpm install` (only if `node_modules/` is missing) and
//! `pnpm run build:web` in `browser/`. pnpm missing → a clear error
//! pointing at the prerequisite.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_WEB");
    if env::var("CARGO_FEATURE_WEB").is_err() {
        return;
    }

    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let browser = manifest.join("browser");

    // Re-run when any React source, the Vite config, or the manifest changes.
    rerun_if_changed(&browser.join("web"));
    rerun_if_changed(&browser.join("package.json5"));
    rerun_if_changed(&browser.join("pnpm-lock.yaml"));
    rerun_if_changed(&browser.join("src"));

    require_pnpm();

    if !browser.join("node_modules").exists() {
        run("pnpm", &["install"], &browser, "pnpm install");
    }

    run("pnpm", &["run", "build:web"], &browser, "pnpm run build:web");

    let dist_index = browser.join("web/dist/index.html");
    if !dist_index.exists() {
        fail(&format!(
            "vite build did not produce {} — check the build:web script in browser/package.json5",
            dist_index.display()
        ));
    }
}

fn rerun_if_changed(path: &Path) {
    println!("cargo:rerun-if-changed={}", path.display());
}

fn require_pnpm() {
    if Command::new("pnpm").arg("--version").output().is_ok() {
        return;
    }
    fail(
        "log-viewer's `web` feature needs pnpm on PATH to build the React app under browser/. \
         Install pnpm (e.g. `corepack enable && corepack prepare pnpm@latest --activate`) and \
         retry, or build without the `web` feature.",
    );
}

fn run(cmd: &str, args: &[&str], dir: &Path, label: &str) {
    let status = Command::new(cmd).args(args).current_dir(dir).status();
    match status {
        Ok(s) if s.success() => {}
        Ok(s) => fail(&format!("{label} failed (exit {:?})", s.code())),
        Err(e) => fail(&format!("running {label}: {e}")),
    }
}

fn fail(msg: &str) -> ! {
    eprintln!("\nerror: {msg}\n");
    std::process::exit(1);
}
