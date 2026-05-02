use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum TaskResult {
    Proceed,
    ShellActionRequired(PathBuf),
}
