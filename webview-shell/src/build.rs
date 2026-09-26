//! For `build.rs`: build the React app so `include_dir!` embeds a current
//! copy.
//!
//! Runs `pnpm install` when `node_modules/` is missing, then the given
//! script, and fails the build if it did not produce `dist_index`.

use std::path::Path;
use std::process::{Command, Stdio};

/// `app_dir` holds `package.json`; `watch` are the paths whose changes
/// should rerun the build script.
pub fn build_web_app(app_dir: &Path, script: &str, dist_index: &Path, watch: &[&Path]) {
    for path in watch {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    println!(
        "cargo:rerun-if-changed={}",
        app_dir.join("package.json").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        app_dir.join("pnpm-lock.yaml").display()
    );

    require_pnpm();
    if !app_dir.join("node_modules").exists() {
        run("pnpm", &["install"], app_dir, "pnpm install");
    }
    run(
        "pnpm",
        &["run", script],
        app_dir,
        &format!("pnpm run {script}"),
    );

    if !dist_index.exists() {
        fail(&format!(
            "the build did not produce {} — check the {script} script in {}",
            dist_index.display(),
            app_dir.join("package.json").display()
        ));
    }
}

/// Draw `emoji` on `background` as a 512-pixel PNG at `out` with
/// appicon-generator, when it is installed, and say whether it did. The
/// image is Apple's emoji font, so it is made at build time rather than
/// committed; a machine without the tool builds an app without an icon.
pub fn emoji_icon(emoji: &str, background: &str, out: &Path) -> bool {
    Command::new("appicon-generator")
        .args([
            "--mode",
            "raw",
            "--size",
            "512",
            "--background",
            background,
            "--output",
        ])
        .arg(out)
        .arg(emoji)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
        && out.exists()
}

fn require_pnpm() {
    if Command::new("pnpm").arg("--version").output().is_ok() {
        return;
    }
    fail(
        "the `web` feature needs pnpm on PATH to build the React app. Install pnpm \
         (e.g. `corepack enable && corepack prepare pnpm@latest --activate`) and retry, \
         or build without the `web` feature.",
    );
}

fn run(cmd: &str, args: &[&str], dir: &Path, label: &str) {
    // Stdin closed: a build script has no one to answer a prompt, and a
    // tool waiting on one hangs the build.
    let status = Command::new(cmd)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .status();
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
