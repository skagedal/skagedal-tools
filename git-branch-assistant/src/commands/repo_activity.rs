use std::env;
use std::path::PathBuf;

use anyhow::Result;

use crate::services::repo_activity_service::{collect_activity, format_entry_lines};

pub fn run(paths: Vec<PathBuf>) -> Result<()> {
    let roots = if paths.is_empty() {
        vec![env::current_dir()?]
    } else {
        paths
    };

    let entries = collect_activity(&roots)?;
    for line in format_entry_lines(&entries) {
        println!("{line}");
    }
    Ok(())
}
