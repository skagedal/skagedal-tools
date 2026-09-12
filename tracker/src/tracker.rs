use crate::config::Config;
use crate::document::Line::{self, OpenShift};
use crate::document::{Day, Document, Parser};
use crate::duration::{format_duration, format_signed_duration};
use crate::paths::TrackerDirs;
use crate::report::{Report, closing_balance};
use chrono::{Datelike, IsoWeek, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta};
use std::env;
use std::fs::OpenOptions;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{fs, io};

pub struct Tracker {
    explicit_weekfile: Option<PathBuf>,
    weekdiff: Option<i32>,
    parser: Parser,
    now: NaiveDateTime,
    dirs: TrackerDirs,
    config: Config,
}

impl Tracker {
    pub fn start_tracking(&self, time_str: Option<String>) {
        let date = self.now.date();
        let time = match time_str {
            Some(time_str) => self.parse_time(&time_str),
            None => self.now.time(),
        };
        let path_buf = self.week_file_created_if_needed(date);
        let document = self
            .read_document(date.iso_week(), path_buf.as_path())
            .unwrap_or_else(|err| {
                panic!("Unexpected error reading document: {}", err);
            });

        // Validate that start time is not before the end time of the previous shift
        if let Err(err) = self.validate_start_time(&document, date, time) {
            eprintln!("{}", err);
            std::process::exit(1);
        }

        let document = match self.document_with_tracking_started(&document, date, time) {
            Ok(doc) => doc,
            Err(DocumentError::TrackerFileAlreadyHasOpenShift {
                date: open_date,
                start_time,
            }) => {
                if open_date == date {
                    eprintln!(
                        "You are already tracking work since {}.",
                        start_time.format("%H:%M")
                    );
                } else {
                    self.offer_to_close_unclosed_shift(open_date);
                }
                std::process::exit(1);
            }
            Err(_) => {
                panic!("Unexpected error starting tracking");
            }
        };

        self.write_day_stdout(&document, date);

        fs::write(path_buf.as_path(), document.to_string())
            .expect("Could not write document to file");
    }

    /// A shift from another day was never closed, so there is nothing sensible
    /// to start right now. Say which day it was, and offer to open the week
    /// file in the editor the way `tracker edit` does.
    fn offer_to_close_unclosed_shift(&self, open_date: NaiveDate) {
        eprintln!("You have a shift from {} that is not closed.", open_date);
        if !io::stdin().is_terminal() {
            eprintln!("Use `tracker edit` to close it.");
            return;
        }
        eprint!("Open editor? (y/n) ");
        let mut answer = String::new();
        if io::stdin().read_line(&mut answer).is_err() {
            return;
        }
        if answer.trim().eq_ignore_ascii_case("y") {
            self.edit_file();
        }
    }

    pub fn stop_tracking(&self) {
        let date = self.now.date();
        let time = self.now.time();
        let path_buf = self.week_tracker_file(date);
        let document = match self.read_document(date.iso_week(), path_buf.as_path()) {
            Ok(document) => document,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                println!("No tracking file for this week has been created.");
                return;
            }
            Err(err) => {
                panic!("Unexpected error reading document: {}", err);
            }
        };

        let document = match self.document_with_tracking_stopped(&document, date, time) {
            Ok(document) => document,
            Err(DocumentError::TrackerFileDoesNotHaveOpenShift) => {
                println!("No session has started.");
                return;
            }
            Err(_) => {
                panic!("Unexpected error stopping tracking");
            }
        };

        self.write_day_stdout(&document, date);

        fs::write(path_buf.as_path(), document.to_string())
            .expect("Could not write document to file");
    }

    pub fn show_weekfile_path(&self) {
        let date = self.now.date();
        let path = self.week_file_created_if_needed(date);
        println!("{}", path.display());
    }

    pub fn edit_file(&self) {
        let path = self.week_file_created_if_needed(self.now.date());

        let editor = env::var("EDITOR").unwrap();
        Command::new(editor)
            .arg(&path)
            .status()
            .expect("Could not open editor");
    }

    pub fn show_report(&self, is_working: bool) {
        let path = self.week_file_created_if_needed(self.now.date());
        let result = fs::read_to_string(path);
        match result {
            Ok(content) => self.process_report_of_content(content, self.now, is_working),
            Err(err) => eprintln!("Error: {}", err),
        }
    }

    fn week_tracker_file(&self, date: NaiveDate) -> PathBuf {
        self.explicit_weekfile
            .clone()
            .unwrap_or_else(|| self.week_tracker_file_for_date(date, self.weekdiff))
    }

    fn week_file_created_if_needed(&self, date: NaiveDate) -> PathBuf {
        let path = self.week_tracker_file(date);
        create_file_if_needed(&path, || self.initial_document(self.active_week(date)));
        path
    }

    /// A new week starts with the balance the latest earlier week ended with.
    /// Weeks without a file in between are skipped, not counted as unworked.
    /// Future weeks get nothing, since the week before them is not over.
    fn initial_document(&self, week: IsoWeek) -> Document {
        if self.explicit_weekfile.is_some() || week > self.now.iso_week() {
            return Document::empty(week);
        }
        let Some((previous_week, path)) = self.latest_week_file_before(week) else {
            return Document::empty(week);
        };
        let content = fs::read_to_string(&path).expect("Could not read previous week file");
        let previous = self.parser.parse_document(previous_week, &content);
        let balance = closing_balance(&previous, &self.config.workweek);
        Document::new(
            week,
            vec![
                Line::Comment {
                    text: format!("balance carried over from {}", format_week(previous_week)),
                },
                Line::DurationShift {
                    text: String::from("balance"),
                    duration: balance,
                },
                Line::Blank,
            ],
            vec![],
        )
    }

    fn latest_week_file_before(&self, week: IsoWeek) -> Option<(IsoWeek, PathBuf)> {
        let entries = fs::read_dir(self.week_files_dir()).ok()?;
        entries
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                let file_week = parse_week_file_name(path.file_name()?.to_str()?)?;
                (file_week < week).then_some((file_week, path))
            })
            .max_by_key(|(file_week, _)| *file_week)
    }

    fn read_document(&self, week: IsoWeek, path: &Path) -> io::Result<Document> {
        match fs::read_to_string(path) {
            Ok(content) => Result::Ok(self.parser.parse_document(week, &content)),
            Err(err) => Result::Err(err),
        }
    }

    fn get_report(&self, content: String, now: NaiveDateTime) -> Report {
        let document = self
            .parser
            .parse_document(self.active_week(now.date()), &content);
        Report::from_document(&document, &now, &self.config.workweek)
    }

    fn process_report_of_content(&self, content: String, now: NaiveDateTime, is_working: bool) {
        let report = self.get_report(content, now);
        if is_working {
            let code = match report.is_ongoing {
                true => 0,
                false => 1,
            };
            std::process::exit(code);
        }

        print!(
            "You have worked {} today",
            format_duration(report.duration_today)
        );
        if report.is_ongoing {
            println!(", ongoing.")
        } else {
            println!(".")
        }
        println!(
            "You have worked {} this week.",
            format_duration(report.duration_week)
        );
        println!("Balance: {}", format_signed_duration(report.balance))
    }

    pub fn document_with_tracking_started(
        &self,
        document: &Document,
        date: NaiveDate,
        time: NaiveTime,
    ) -> Result<Document, DocumentError> {
        if let Some((open_date, start_time)) = document.open_shift() {
            return Err(DocumentError::TrackerFileAlreadyHasOpenShift {
                date: open_date,
                start_time,
            });
        }
        if let Some(day) = document.days.iter().find(|day| day.date.eq(&date)) {
            return Ok(
                document.replacing_day(date, day.adding_shift(OpenShift { start_time: time }))
            );
        }
        Ok(document.inserting_day(Day::create(date, vec![OpenShift { start_time: time }])))
    }

    pub fn document_with_tracking_stopped(
        &self,
        document: &Document,
        date: NaiveDate,
        time: NaiveTime,
    ) -> Result<Document, DocumentError> {
        if !document.has_open_shift() {
            return Err(DocumentError::TrackerFileDoesNotHaveOpenShift);
        }
        if let Some(day) = document.days.iter().find(|day| day.date.eq(&date)) {
            return Ok(document.replacing_day(date, day.closing_shift(time)));
        }
        Err(DocumentError::TrackerFileDoesNotHaveOpenShift)
    }

    fn write_day_stdout(&self, document: &Document, date: NaiveDate) {
        let day = document
            .get_day(date)
            .expect("this should be called right after day is modified");
        print!("{}", day)
    }

    fn parse_time(&self, time_str: &str) -> NaiveTime {
        let parts: Vec<&str> = time_str.split(':').collect();
        if parts.len() != 2 {
            eprintln!("Invalid time format. Expected HH:MM (e.g., 08:30)");
            std::process::exit(1);
        }

        let hour = parts[0].parse::<u32>().unwrap_or_else(|_| {
            eprintln!("Invalid hour in time format. Expected HH:MM (e.g., 08:30)");
            std::process::exit(1);
        });

        let minute = parts[1].parse::<u32>().unwrap_or_else(|_| {
            eprintln!("Invalid minute in time format. Expected HH:MM (e.g., 08:30)");
            std::process::exit(1);
        });

        NaiveTime::from_hms_opt(hour, minute, 0).unwrap_or_else(|| {
            eprintln!("Invalid time. Hour must be 0-23 and minute must be 0-59");
            std::process::exit(1);
        })
    }

    fn validate_start_time(
        &self,
        document: &Document,
        date: NaiveDate,
        time: NaiveTime,
    ) -> Result<(), String> {
        // Find the last closed shift on the same date or before
        let mut last_end_time: Option<(NaiveDate, NaiveTime)> = None;

        for day in &document.days {
            if day.date > date {
                break;
            }

            for line in &day.lines {
                match line {
                    Line::ClosedShift { stop_time, .. } => {
                        last_end_time = Some((day.date, *stop_time));
                    }
                    Line::SpecialShift { stop_time, .. } => {
                        last_end_time = Some((day.date, *stop_time));
                    }
                    _ => {}
                }
            }
        }

        if let Some((last_date, last_time)) = last_end_time {
            // If the last shift was on the same day
            if last_date == date && time < last_time {
                return Err(format!(
                    "Start time {} is before the end time {} of the previous shift on the same day",
                    time.format("%H:%M"),
                    last_time.format("%H:%M")
                ));
            }
            // If the last shift was on a previous day, we don't need to validate
            // (shifts can start earlier on a new day)
        }

        Ok(())
    }

    fn active_week(&self, date: NaiveDate) -> IsoWeek {
        self.weekdiff
            .map(|d| date + TimeDelta::try_days(d as i64 * 7).unwrap())
            .unwrap_or(date)
            .iso_week()
    }

    fn week_tracker_file_for_date(&self, date: NaiveDate, weekdiff: Option<i32>) -> PathBuf {
        let date = weekdiff
            .map(|d| date + TimeDelta::try_days(d as i64 * 7).unwrap())
            .unwrap_or(date);

        self.week_files_dir()
            .join(date.format("%G-W%V.txt").to_string())
    }

    fn week_files_dir(&self) -> PathBuf {
        self.dirs.data_dir().join("week-files")
    }
}

#[derive(Debug, Clone)]
pub enum DocumentError {
    TrackerFileAlreadyHasOpenShift {
        date: NaiveDate,
        start_time: NaiveTime,
    },
    TrackerFileDoesNotHaveOpenShift,
}

impl Tracker {
    pub fn builder(now: NaiveDateTime, dirs: TrackerDirs) -> TrackerBuilder {
        TrackerBuilder::default().now(now).dirs(dirs)
    }
}

#[derive(Default)]
pub struct TrackerBuilder {
    explicit_weekfile: Option<PathBuf>,
    weekdiff: Option<i32>,
    now: Option<NaiveDateTime>,
    dirs: Option<TrackerDirs>,
    config: Option<Config>,
}

impl TrackerBuilder {
    pub fn explicit_weekfile(mut self, explicit_weekfile: Option<PathBuf>) -> Self {
        self.explicit_weekfile = explicit_weekfile;
        self
    }

    pub fn weekdiff(mut self, weekdiff: Option<i32>) -> Self {
        self.weekdiff = weekdiff;
        self
    }

    pub fn now(mut self, now: NaiveDateTime) -> Self {
        self.now = Some(now);
        self
    }

    pub fn dirs(mut self, dirs: TrackerDirs) -> Self {
        self.dirs = Some(dirs);
        self
    }

    pub fn config(mut self, config: Config) -> Self {
        self.config = Some(config);
        self
    }

    pub fn build(self) -> Tracker {
        Tracker {
            explicit_weekfile: self.explicit_weekfile,
            weekdiff: self.weekdiff,
            parser: Parser::new(),
            now: self.now.expect("now value required"),
            dirs: self.dirs.expect("dirs value expected"),
            config: self.config.unwrap_or_default(),
        }
    }
}

// Week tracker file

fn create_file_if_needed(path: &Path, initial_document: impl FnOnce() -> Document) {
    if let Some(parent_path) = path.parent() {
        fs::create_dir_all(parent_path).unwrap_or_else(|err| eprintln!("Error: {}", err));
    }
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => {
            file.write_all(initial_document().to_string().as_bytes())
                .expect("Could not write initial document to file");
        }
        Err(err) => {
            if err.kind() != io::ErrorKind::AlreadyExists {
                eprintln!("Error: {}", err);
            }
        }
    }
}

fn format_week(week: IsoWeek) -> String {
    format!("{}-W{:02}", week.year(), week.week())
}

/// The week of a file named like `2024-W04.txt`.
fn parse_week_file_name(name: &str) -> Option<IsoWeek> {
    let (year, week) = name.strip_suffix(".txt")?.split_once("-W")?;
    if week.len() != 2 {
        return None;
    }
    NaiveDate::from_isoywd_opt(year.parse().ok()?, week.parse().ok()?, chrono::Weekday::Mon)
        .map(|date| date.iso_week())
}

#[cfg(test)]
mod tests;
