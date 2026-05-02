use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub shell: String,
    pub when: String,
    #[serde(rename = "if")]
    pub iff: Option<String>,
    pub directory: Option<String>,
}

pub fn read_tasks(tasks_file: PathBuf) -> Vec<Task> {
    let file = std::fs::File::open(tasks_file).expect("Could not open tasks.yml");
    serde_yaml::from_reader(file).expect("Could not read tasks.yml")
}
