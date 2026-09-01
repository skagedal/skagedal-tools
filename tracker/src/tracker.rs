use crate::config::Config;
use crate::document::Line::{self, OpenShift};
use crate::document::{Day, Document, Parser};
use crate::duration::{format_duration, format_signed_duration};
use crate::paths::TrackerDirs;
use crate::report::Report;
use chrono::{Datelike, IsoWeek, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta};
use std::env;
use std::fs::OpenOptions;
use std::io::Write;
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
        let path_buf = if self.config.experimental_features.auto_transfer_balance {
            week_tracker_file_create_if_needed_with_transfer(
                date.iso_week(),
                self.week_tracker_file(date),
                self.week_to_transfer_from(date),
            )
        } else {
            week_tracker_file_create_if_needed(date.iso_week(), self.week_tracker_file(date))
        };
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
            Err(DocumentError::TrackerFileAlreadyHasOpenShift) => {
                eprintln!(
                    "You are already tracking. Use `tracker stop` to end the current shift. If you forgot to stop tracking earlier, use `tracker edit`."
                );
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
        let path =
            week_tracker_file_create_if_needed(date.iso_week(), self.week_tracker_file(date));
        println!("{}", path.display());
    }

    pub fn edit_file(&self) {
        let path = week_tracker_file_create_if_needed(
            self.now.iso_week(),
            self.week_tracker_file(self.now.date()),
        );

        let editor = env::var("EDITOR").unwrap();
        Command::new(editor)
            .arg(&path)
            .status()
            .expect("Could not open editor");
    }

    pub fn show_report(&self, is_working: bool) {
        let path = week_tracker_file_create_if_needed(
            self.active_week(self.now.date()),
            self.week_tracker_file(self.now.date()),
        );
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

    // transfer only happens from previous week when no explicit week file or week diff has been set
    fn week_to_transfer_from(&self, date: NaiveDate) -> Option<IsoWeek> {
        if self.explicit_weekfile.is_none() && self.weekdiff.is_none() {
            Some((date - TimeDelta::try_days(7).unwrap()).iso_week())
        } else {
            None
        }
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
        if document.has_open_shift() {
            return Err(DocumentError::TrackerFileAlreadyHasOpenShift);
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

        self.dirs
            .data_dir()
            .join("week-files")
            .join(date.format("%G-W%V.txt").to_string())
    }
}

#[derive(Debug, Clone)]
pub enum DocumentError {
    TrackerFileAlreadyHasOpenShift,
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

fn week_tracker_file_create_if_needed_with_transfer(
    week: IsoWeek,
    path: PathBuf,
    last_week: Option<IsoWeek>,
) -> PathBuf {
    // Create parents if needed
    if let Some(parent_path) = path.parent() {
        fs::create_dir_all(parent_path).unwrap_or_else(|err| eprintln!("Error: {}", err));
    }

    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut file) => {
            let initial_document = default_document(week, last_week);
            file.write_all(initial_document.to_string().as_bytes())
                .expect("Could not write example document to file");
        }
        Err(err) => {
            if err.kind() != io::ErrorKind::AlreadyExists {
                eprintln!("Error: {}", err);
            }
        }
    }

    path
}

fn default_document(week: IsoWeek, last_week: Option<IsoWeek>) -> Document {
    if let Some(_last_week) = last_week {
        // let content = fs::read_to_string(path).expect("Could not read last week file");
        // let last_week_document = Parser::new().parse_document(last_week, &content);
        // return Document::empty_with_balance(last_week_document);
        return Document::empty(week);
    }
    Document::empty(week)
}

fn week_tracker_file_create_if_needed(week: IsoWeek, path: PathBuf) -> PathBuf {
    week_tracker_file_create_if_needed_with_transfer(week, path, None)
}

#[cfg(test)]
mod tests;
