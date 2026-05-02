use std::fs;
use std::path::Path;

use anyhow::Result;

use crate::env::ProcessEnvironment;
use crate::fs_utils::expand_tilde;

#[derive(Default)]
pub struct Repository;

impl Repository {
    pub fn new() -> Self {
        Self
    }

    pub fn set_suggested_directory(&self, suggested_directory: &Path) -> Result<()> {
        if let Some(path_str) = ProcessEnvironment::suggested_cd_file() {
            let target_path = expand_tilde(&path_str);
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(
                target_path,
                suggested_directory_to_string(suggested_directory),
            )?;
        }
        Ok(())
    }
}

fn suggested_directory_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
