use std::path::PathBuf;

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use temp_dir::TempDir;
use tracker::{
    paths::TrackerDirs,
    tracker::{Tracker, TrackerBuilder},
};

struct TrackerTestContext {
    #[allow(dead_code)]
    tempdir: TempDir,
    builder: TrackerBuilder,
}

fn test_data() -> PathBuf {
    [env!("CARGO_MANIFEST_DIR"), "tests", "data"]
        .iter()
        .collect()
}

pub fn naive_date_time(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> NaiveDateTime {
    NaiveDateTime::new(naive_date(year, month, day), naive_time(hour, minute))
}

pub fn naive_date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
}

pub fn naive_time(hour: u32, minute: u32) -> NaiveTime {
    NaiveTime::from_hms_opt(hour, minute, 0).unwrap()
}

fn ctx() -> TrackerTestContext {
    let tempdir = TempDir::new().unwrap();
    let dirs = TrackerDirs::fixed(tempdir.path());
    let builder = Tracker::builder(naive_date_time(2023, 12, 2, 12, 0), dirs);

    TrackerTestContext { tempdir, builder }
}

#[test]
fn read_file_and_report() {
    // This will print:
    //
    // You have worked 0 h 0 m today.
    // You have worked 40 h 54 m this week.
    // Balance: 8 h 36 m
    let ctx = ctx();
    let tracker = ctx
        .builder
        .explicit_weekfile(Some(test_data().join("2024-W04.txt")))
        .build();
    tracker.show_report(false)
}

#[test]
fn no_op_test() {
    let ctx = ctx();
    let _tracker = ctx.builder.build();
    assert_eq!(true, true);
}
