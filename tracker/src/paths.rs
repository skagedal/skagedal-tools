use std::path::{Path, PathBuf};

const TOOL: &str = "tracker";

pub struct TrackerDirs {
    config_dir: PathBuf,
    data_dir: PathBuf,
}

impl TrackerDirs {
    pub fn real() -> TrackerDirs {
        TrackerDirs {
            config_dir: skagedal_dirs::config_dir(TOOL),
            data_dir: skagedal_dirs::data_dir(TOOL),
        }
    }

    pub fn fixed(path: &Path) -> TrackerDirs {
        TrackerDirs {
            config_dir: path.join("config").to_path_buf(),
            data_dir: path.join("data").to_path_buf(),
        }
    }

    pub fn config_dir(&self) -> &Path {
        self.config_dir.as_path()
    }

    pub fn data_dir(&self) -> &Path {
        self.data_dir.as_path()
    }
}
