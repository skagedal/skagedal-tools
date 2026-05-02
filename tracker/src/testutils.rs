/// Helpers to be used in the test suite only.
use chrono::{Datelike, IsoWeek, NaiveDate, NaiveDateTime, NaiveTime};

pub fn naive_date_time(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> NaiveDateTime {
    NaiveDateTime::new(naive_date(year, month, day), naive_time(hour, minute))
}

pub fn naive_date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
}

pub fn naive_time(hour: u32, minute: u32) -> NaiveTime {
    NaiveTime::from_hms_opt(hour, minute, 0).unwrap()
}

pub fn iso_date(year: i32, week: u32, weekday: chrono::Weekday) -> NaiveDate {
    NaiveDate::from_isoywd_opt(year, week, weekday).unwrap()
}

pub fn iso_week(year: i32, week: u32) -> IsoWeek {
    NaiveDate::from_isoywd_opt(year, week, chrono::Weekday::Mon)
        .unwrap()
        .iso_week()
}
