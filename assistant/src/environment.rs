use filetime::FileTime;
use std::fs;
use std::fs::File;
use std::io::{ErrorKind, Read, Write};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

const TOOL: &str = "assistant";

pub struct Environment {
    config_dir: PathBuf,
    data_dir: PathBuf,
}

impl Environment {
    pub fn read() -> Environment {
        Environment {
            config_dir: skagedal_dirs::config_dir(TOOL),
            data_dir: skagedal_dirs::data_dir(TOOL),
        }
    }

    pub fn tasks_yml(&self) -> PathBuf {
        self.config_dir.join("tasks.yml")
    }

    pub fn when_did_we_last_do(&self, task_id: &str) -> Option<SystemTime> {
        match fs::metadata(self.path_for_task(task_id)) {
            Ok(metadata) => Some(metadata.modified().unwrap()),
            Err(error) => {
                if error.kind() == ErrorKind::NotFound {
                    None
                } else {
                    panic!("Error: {}", error)
                }
            }
        }
    }

    pub fn mark_as_done(&self, task_id: &str) {
        let path_buf = self.path_for_task(task_id);
        let _ = File::create(path_buf);
        // filetime::set_file_mtime(path_buf, FileTime::now())
        //     .expect("Could not set file time");
    }

    pub fn mark_as_done_in_an_hour(&self, task_id: &str) {
        let path_buf = self.path_for_task(task_id);
        let _ = File::create(&path_buf);
        filetime::set_file_mtime(path_buf, Self::in_an_hour()).expect("Could not set file time");
    }

    fn in_an_hour() -> FileTime {
        FileTime::from_system_time(
            SystemTime::now()
                .checked_add(Duration::from_secs(3600))
                .unwrap(),
        )
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }

    fn path_for_task(&self, task_id: &str) -> PathBuf {
        self.data_dir.join(format!("{}.task", task_id))
    }

    pub fn clear_failed_task(&self) {
        let path = self.data_dir.join("last_failed_task");
        let _ = fs::remove_file(path);
    }

    pub fn record_failed_task(&self, task_id: &str) {
        let path = self.data_dir.join("last_failed_task");
        if let Ok(mut file) = File::create(&path) {
            let _ = file.write_all(task_id.as_bytes());
        }
    }

    pub fn get_latest_failed_task(&self) -> Option<String> {
        let path = self.data_dir.join("last_failed_task");
        let mut file = File::open(path).ok()?;
        let mut contents = String::new();
        file.read_to_string(&mut contents).ok()?;
        Some(contents.trim().to_string())
    }
}
