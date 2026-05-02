use filetime::FileTime;
use std::fs;
use std::fs::File;
use std::io::{ErrorKind, Read, Write};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

pub struct Environment {
    pub home: PathBuf,
}

#[derive(Debug)]
pub struct Error {
    #[allow(dead_code)]
    message: String,
}

impl Error {
    fn could_not_find_home() -> Error {
        Error {
            message: String::from("Could not find home directory, is HOME set?"),
        }
    }
}

impl Environment {
    pub fn read() -> Result<Environment, Error> {
        match dirs::home_dir() {
            None => Err(Error::could_not_find_home()),
            Some(path) => Ok(Environment { home: path }),
        }
    }

    pub fn tasks_yml(&self) -> PathBuf {
        self.assistant_dir().join("tasks.yml")
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
        self.assistant_dir().join("logs")
    }

    fn path_for_task(&self, task_id: &str) -> PathBuf {
        self.data_dir().join(format!("{}.task", task_id))
    }

    fn data_dir(&self) -> PathBuf {
        self.assistant_dir().join("data")
    }
    fn assistant_dir(&self) -> PathBuf {
        self.home.join(".simons-assistant")
    }

    pub fn clear_failed_task(&self) {
        let path = self.data_dir().join("last_failed_task");
        let _ = fs::remove_file(path);
    }

    pub fn record_failed_task(&self, task_id: &str) {
        let path = self.data_dir().join("last_failed_task");
        if let Ok(mut file) = File::create(&path) {
            let _ = file.write_all(task_id.as_bytes());
        }
    }

    pub fn get_latest_failed_task(&self) -> Option<String> {
        let path = self.data_dir().join("last_failed_task");
        let mut file = File::open(path).ok()?;
        let mut contents = String::new();
        file.read_to_string(&mut contents).ok()?;
        Some(contents.trim().to_string())
    }
}
