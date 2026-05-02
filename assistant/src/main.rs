use log::debug;
use once_cell::sync::Lazy;
use std::process::Command;
use std::time::SystemTime;
use std::{env, io};

use regex::Regex;

use tasks::Task;

use crate::WhenExpression::{Always, EveryNDays, EveryNHours, Never};
use crate::duechecker::{Due, check};
use crate::environment::Environment;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{Shell, generate};
use flexi_logger::{Duplicate, FileSpec, Logger};

mod duechecker;
mod environment;
mod tasks;

/// Perform recurring tasks and tell you what to do next
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
#[command(arg_required_else_help(true))]
struct Args {
    #[clap(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Tell you what to do next
    Next,
    /// Edit tasks file
    Edit,
    /// List tasks
    List,
    /// Snooze a specific task (or the latest failed task if no ID provided)
    Snooze { id: Option<String> },
    /// Run a specific task regardless of whether it is due
    Run { id: String },
    /// Generate autocompletion script
    Completions { shell: Shell },
}

fn main() {
    let environment = Environment::read().expect("Unexpected environment");

    Logger::try_with_env_or_str("debug")
        .expect("Could not create logger")
        .log_to_file(FileSpec::default().directory(environment.logs_dir()))
        .duplicate_to_stderr(Duplicate::Warn)
        .start()
        .unwrap_or_else(|e| panic!("Logger initialization failed with {:?}", e));

    let args = Args::parse();
    match args.command {
        Some(Commands::Next) => next(&environment),
        Some(Commands::Edit) => edit(&environment),
        Some(Commands::List) => list(&environment),
        Some(Commands::Snooze { id }) => snooze(&environment, id),
        Some(Commands::Run { id }) => run(&environment, id),
        Some(Commands::Completions { shell }) => completions(&environment, shell),
        None => panic!("Because of arg_required_else_help(true), this should never happen"),
    }
}

// Top level command handlers

fn next(environment: &Environment) {
    let tasks = tasks::read_tasks(environment.tasks_yml());
    handle_tasks(environment, tasks);
}

fn edit(environment: &Environment) {
    let path = environment.tasks_yml();
    let editor = env::var("EDITOR").unwrap();
    Command::new(editor)
        .arg(&path)
        .status()
        .expect("Could not open editor");
}

fn list(environment: &Environment) {
    let tasks = tasks::read_tasks(environment.tasks_yml());
    list_tasks(&environment, tasks);
}

fn completions(_environment: &Environment, shell: Shell) {
    let mut cmd = Args::command();
    generate(shell, &mut cmd, "assistant", &mut io::stdout());
}

fn list_tasks(_environment: &&Environment, tasks: Vec<Task>) {
    for task in tasks.iter() {
        println!("{}", task.id)
    }
}

fn snooze(environment: &Environment, task_id: Option<String>) {
    let id = match task_id {
        Some(id) => id,
        None => match environment.get_latest_failed_task() {
            Some(id) => {
                println!("No task ID provided, snoozing latest failed task: {}", id);
                id
            }
            None => {
                eprintln!("No task ID provided and no failed task recorded");
                return;
            }
        },
    };
    println!("Snoozing {} for 1 hour", id);
    environment.mark_as_done_in_an_hour(id.as_str())
}

fn run(environment: &Environment, task_id: String) {
    let tasks = tasks::read_tasks(environment.tasks_yml());
    match tasks.iter().find(|t| t.id == task_id) {
        None => eprintln!("No task found with id: {}", task_id),
        Some(task) => match do_the_task(&task.shell, &task.directory) {
            TaskResult::Proceed => environment.mark_as_done(task.id.as_str()),
            TaskResult::ActionRequired => {
                environment.record_failed_task(task.id.as_str());
            }
        },
    }
}

// Handle tasks ("next")

fn handle_tasks(environment: &Environment, tasks: Vec<Task>) {
    environment.clear_failed_task();
    for task in tasks.iter() {
        let Task {
            id,
            shell,
            when,
            iff,
            directory,
        } = task;
        match handle_task(environment, id, shell, when, iff, directory) {
            TaskResult::Proceed => {}
            TaskResult::ActionRequired => {
                debug!("Action required, exiting");
                environment.record_failed_task(id.as_str());
                return;
            }
        }
    }
}

fn handle_task(
    environment: &Environment,
    id: &str,
    shell: &str,
    when_string: &str,
    if_string: &Option<String>,
    directory: &Option<String>,
) -> TaskResult {
    let when = WhenExpression::from_string(when_string);
    let time = environment.when_did_we_last_do(id);
    let Due { is_due, reason } = check(&when, &time, &SystemTime::now());
    if is_due && optional_condition_matches(if_string) {
        debug!(
            "{} was last done: {:?} - it is due because {}",
            id, time, reason
        );
        match do_the_task(shell, directory) {
            TaskResult::Proceed => {
                environment.mark_as_done(id);
                TaskResult::Proceed
            }
            TaskResult::ActionRequired => TaskResult::ActionRequired,
        }
    } else {
        debug!("{} was last done: {:?} - it is not due", id, time);
        TaskResult::Proceed
    }
}

fn optional_condition_matches(if_string: &Option<String>) -> bool {
    if_string.as_deref().map(condition_matches).unwrap_or(true)
}

fn condition_matches(if_string: &str) -> bool {
    let mut cmd = shell_command(if_string, &None);

    match cmd.status() {
        Ok(exit_status) => match exit_status.code() {
            None => panic!("No exit status"),
            Some(0) => {
                debug!("condition \"{}\" evaulated to true", if_string);
                true
            }
            Some(x) => {
                debug!(
                    "condition \"{}\" evaulated to false, return code {}",
                    if_string, x
                );
                false
            }
        },
        Err(err) => {
            eprintln!("Error running condition: {}", if_string);
            eprintln!("{}", err);
            false
        }
    }
}

fn do_the_task(shell: &str, directory: &Option<String>) -> TaskResult {
    let mut cmd = shell_command(shell, directory);

    match cmd.status() {
        Ok(exit_status) => match exit_status.code() {
            None => panic!("No exit status"),
            Some(0) => TaskResult::Proceed,
            Some(_) => TaskResult::ActionRequired,
        },
        Err(err) => {
            eprintln!("Error running command: {}", shell);
            eprintln!("{}", err);
            TaskResult::ActionRequired
        }
    }
}

fn shell_command(shell: &str, directory: &Option<String>) -> Command {
    let mut cmd = Command::new("bash");
    cmd.arg("-c").arg(shell);

    if let Some(dir) = directory {
        let expanded = shellexpand::tilde(dir);
        cmd.current_dir(expanded.into_owned());
    }

    cmd
}

enum TaskResult {
    Proceed,
    ActionRequired,
    // ShellActionRequired(PathBuf)
}

#[derive(PartialEq, Debug)]
pub enum WhenExpression {
    Never,
    Always,
    EveryNDays { n: i32 },
    EveryNHours { n: i32 },
}

impl WhenExpression {
    fn from_string(str: &str) -> WhenExpression {
        static RE_DAYS: Lazy<Regex> =
            Lazy::new(|| Regex::new("^every (?P<n>[0-9]+) days$").unwrap());
        static RE_HOURS: Lazy<Regex> =
            Lazy::new(|| Regex::new("^every (?P<n>[0-9]+) hours$").unwrap());

        if str == "never" {
            Never
        } else if str == "always" {
            Always
        } else if str == "daily" {
            EveryNDays { n: 1 }
        } else if str == "weekly" {
            EveryNDays { n: 7 }
        } else if let Some(m) = RE_DAYS.captures(str) {
            EveryNDays {
                n: m.name("n").unwrap().as_str().parse::<i32>().unwrap(),
            }
        } else if str == "hourly" {
            EveryNHours { n: 1 }
        } else if let Some(m) = RE_HOURS.captures(str) {
            EveryNHours {
                n: m.name("n").unwrap().as_str().parse::<i32>().unwrap(),
            }
        } else {
            panic!("We should handle this better")
        }
    }
}

// List tasks

// Tests

#[cfg(test)]
mod tests {
    // use crate::parse_when_expression;
    use crate::WhenExpression;
    use crate::WhenExpression::{Always, EveryNDays, Never};

    #[test]
    fn good_when_exp() {
        assert_eq!(Always, WhenExpression::from_string("always"));
        assert_eq!(Never, WhenExpression::from_string("never"));
        assert_eq!(EveryNDays { n: 1 }, WhenExpression::from_string("daily"));
        assert_eq!(
            EveryNDays { n: 5 },
            WhenExpression::from_string("every 5 days")
        );
    }

    #[test]
    #[should_panic]
    fn bad_when_exp() {
        let _ = WhenExpression::from_string("asdf");
    }
}
