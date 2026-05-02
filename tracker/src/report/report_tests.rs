use chrono::{Datelike, NaiveDate};

use crate::{
    config::WorkWeekConfig,
    document::{Day, Document, Line},
    report::Report,
    testutils::{iso_date, iso_week, naive_date, naive_date_time, naive_time},
};

#[test]
fn empty_report() {
    let now = naive_date_time(2023, 12, 18, 12, 0);
    let week = iso_week(2023, 51);
    let document = Document::new(week, vec![], vec![]);
    assert_eq!(
        Report {
            duration_today: chrono::Duration::hours(0),
            duration_week: chrono::Duration::hours(0),
            is_ongoing: false,
            balance: chrono::Duration::hours(-8)
        },
        Report::from_document(&document, &now, &WorkWeekConfig::default())
    )
}

#[test]
fn simple_report() {
    let document = Document::new(
        iso_week(2023, 51),
        vec![],
        vec![Day {
            date: naive_date(2023, 12, 18), // a monday
            lines: vec![Line::ClosedShift {
                start_time: naive_time(8, 0),
                stop_time: naive_time(12, 0),
            }],
        }],
    );
    let now = naive_date_time(2023, 12, 18, 12, 0);
    assert_eq!(
        Report {
            duration_today: chrono::Duration::hours(4),
            duration_week: chrono::Duration::hours(4),
            is_ongoing: false,
            balance: chrono::Duration::hours(-4)
        },
        Report::from_document(&document, &now, &WorkWeekConfig::default())
    )
}

#[test]
fn special_days_are_counted() {
    let document = Document::new(
        naive_date(2023, 12, 18).iso_week(),
        vec![],
        vec![Day {
            date: naive_date(2023, 12, 18), // a monday
            lines: vec![Line::SpecialDay {
                text: String::from("vacation"),
            }],
        }],
    );
    let now = naive_date_time(2023, 12, 18, 12, 0);
    assert_eq!(
        Report {
            duration_today: chrono::Duration::hours(8),
            duration_week: chrono::Duration::hours(8),
            is_ongoing: false,
            balance: chrono::Duration::hours(0)
        },
        Report::from_document(&document, &now, &WorkWeekConfig::default())
    )
}

#[test]
fn special_shifts_are_counted() {
    let document = Document::new(
        naive_date(2023, 12, 18).iso_week(),
        vec![],
        vec![Day {
            date: naive_date(2023, 12, 18), // a monday
            lines: vec![Line::SpecialShift {
                text: String::from("vacation"),
                start_time: naive_time(11, 10),
                stop_time: naive_time(11, 50),
            }],
        }],
    );
    let now = naive_date_time(2023, 12, 18, 12, 0);
    assert_eq!(
        Report {
            duration_today: chrono::Duration::minutes(40),
            duration_week: chrono::Duration::minutes(40),
            is_ongoing: false,
            balance: chrono::Duration::minutes(40 - 8 * 60)
        },
        Report::from_document(&document, &now, &WorkWeekConfig::default())
    )
}

#[test]
fn shifts_are_summed_correctly() {
    let document = Document::new(
        naive_date(2023, 12, 18).iso_week(),
        vec![],
        vec![
            Day {
                date: naive_date(2023, 12, 18), // a monday
                lines: vec![
                    Line::SpecialShift {
                        text: String::from("vacation"),
                        start_time: naive_time(11, 10),
                        stop_time: naive_time(11, 50),
                    },
                    Line::ClosedShift {
                        start_time: naive_time(13, 5),
                        stop_time: naive_time(13, 10),
                    },
                ],
            },
            Day {
                date: naive_date(2023, 12, 19), // a tuesday
                lines: vec![Line::ClosedShift {
                    start_time: naive_time(8, 0),
                    stop_time: naive_time(12, 0),
                }],
            },
        ],
    );
    let now = naive_date_time(2023, 12, 19, 12, 0);
    assert_eq!(
        Report {
            duration_today: chrono::Duration::hours(4),
            duration_week: chrono::Duration::minutes(285),
            is_ongoing: false,
            balance: chrono::Duration::minutes(285 - 2 * 8 * 60)
        },
        Report::from_document(&document, &now, &WorkWeekConfig::default())
    )
}

#[test]
fn report_for_earlier_week() {
    fn full_day(date: NaiveDate) -> Day {
        Day {
            date,
            lines: vec![Line::ClosedShift {
                start_time: naive_time(8, 0),
                stop_time: naive_time(16, 0),
            }],
        }
    }
    let document = Document::new(
        iso_week(2023, 50),
        vec![],
        vec![
            full_day(iso_date(2023, 50, chrono::Weekday::Mon)),
            full_day(iso_date(2023, 50, chrono::Weekday::Tue)),
            full_day(iso_date(2023, 50, chrono::Weekday::Wed)),
            full_day(iso_date(2023, 50, chrono::Weekday::Thu)),
            full_day(iso_date(2023, 50, chrono::Weekday::Fri)),
        ],
    );
    // Next week, on wednesday, we're viewing the report.
    let now = naive_date_time(2023, 12, 20, 12, 0);
    assert_eq!(
        Report {
            duration_today: chrono::Duration::hours(0),
            duration_week: chrono::Duration::hours(40),
            is_ongoing: false,
            balance: chrono::Duration::hours(0)
        },
        Report::from_document(&document, &now, &WorkWeekConfig::default())
    )
}
