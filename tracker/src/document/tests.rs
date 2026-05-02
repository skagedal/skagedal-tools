use crate::document::Line::{
    Blank, ClosedShift, Comment, DayHeader, DurationShift, OpenShift, SpecialDay, SpecialShift,
};
use crate::document::{Day, Document, Parser};
use chrono::{Datelike, Duration, IsoWeek, NaiveDate, NaiveTime, TimeDelta};

#[test]
fn read_line() {
    let parser = Parser::new();

    assert_eq!(
        Option::Some(Comment {
            text: String::from("hello")
        }),
        parser.parse_line("# hello")
    );

    assert_eq!(
        Option::Some(DayHeader {
            date: NaiveDate::from_ymd_opt(2021, 9, 13).unwrap()
        }),
        parser.parse_line("[monday 2021-09-13]")
    );

    assert_eq!(
        Option::Some(OpenShift {
            start_time: time_hm(8, 12)
        }),
        parser.parse_line("* 08:12-")
    );

    assert_eq!(
        Option::Some(ClosedShift {
            start_time: time_hm(8, 24),
            stop_time: time_hm(9, 12)
        }),
        parser.parse_line("* 08:24-09:12")
    );

    assert_eq!(
        Option::Some(SpecialDay {
            text: String::from("hello")
        }),
        parser.parse_line("* hello")
    );

    assert_eq!(
        Option::Some(SpecialShift {
            text: String::from("VAB"),
            start_time: time_hm(13, 5),
            stop_time: time_hm(20, 2)
        }),
        parser.parse_line("* VAB 13:05-20:02")
    );

    assert_eq!(
        Option::Some(DurationShift {
            text: String::from("balance"),
            duration: TimeDelta::try_hours(20).unwrap()
        }),
        parser.parse_line("* balance 20 h 0 m")
    );

    assert_eq!(
        Option::Some(DurationShift {
            text: String::from("balance"),
            duration: TimeDelta::try_minutes(-5 * 60 - 6).unwrap()
        }),
        parser.parse_line("* balance -5h -6m")
    );

    assert_eq!(Option::Some(Blank), parser.parse_line(""));
}

#[test]
fn serde_duration() {
    let parser = Parser::new();

    let line = "* carry 1h 30m";
    let parsed = parser.parse_line(line).unwrap();
    let serialized = parsed.to_string();
    assert_eq!(line, serialized);

    let line = "* carry -2h -15m";
    let parsed = parser.parse_line(line).unwrap();
    let serialized = parsed.to_string();
    assert_eq!(line, serialized);
}

#[test]
fn deserialize() {
    let week = example_1_week();
    let serialized_document = example_1_text();
    let document = example_1_document();

    let parser = Parser::new();
    let parsed = parser.parse_document(week, &serialized_document);
    assert_eq!(document, parsed)
}

#[test]
fn serialize() {
    let serialized_document = example_1_text();
    let document = example_1_document();

    assert_eq!(serialized_document, document.to_string())
}

#[test]
fn replacing_day_that_does_not_exist() {
    let document = Document {
        week: NaiveDate::from_ymd_opt(2020, 7, 13).unwrap().iso_week(),
        preamble: vec![],
        days: vec![],
    };
    let new_document = document.replacing_day(
        NaiveDate::from_ymd_opt(2020, 7, 13).unwrap(),
        Day {
            date: NaiveDate::from_ymd_opt(2020, 7, 13).unwrap(),
            lines: vec![],
        },
    );
    assert_eq!(document, new_document)
}

// Helpers

fn time_hm(hour: u32, minute: u32) -> NaiveTime {
    NaiveTime::from_hms_opt(hour, minute, 0).unwrap()
}

// Test data

fn example_1_document() -> Document {
    Document {
        week: example_1_week(),
        preamble: vec![
            Comment {
                text: String::from("Preamble"),
            },
            DurationShift {
                text: String::from("carry"),
                duration: Duration::minutes(70),
            },
            DurationShift {
                text: String::from("negativecarry"),
                duration: Duration::minutes(-130),
            },
            Blank,
        ],
        days: vec![
            Day {
                date: NaiveDate::from_ymd_opt(2020, 7, 13).unwrap(),
                lines: vec![
                    SpecialDay {
                        text: String::from("Vacation"),
                    },
                    Comment {
                        text: String::from("Came back from Jämtland"),
                    },
                    Blank,
                ],
            },
            Day {
                date: NaiveDate::from_ymd_opt(2020, 7, 14).unwrap(),
                lines: vec![
                    ClosedShift {
                        start_time: time_hm(8, 32),
                        stop_time: time_hm(12, 2),
                    },
                    ClosedShift {
                        start_time: time_hm(12, 30),
                        stop_time: time_hm(13, 1),
                    },
                    ClosedShift {
                        start_time: time_hm(13, 45),
                        stop_time: time_hm(18, 3),
                    },
                    Blank,
                ],
            },
            Day {
                date: NaiveDate::from_ymd_opt(2020, 7, 15).unwrap(),
                lines: vec![
                    ClosedShift {
                        start_time: time_hm(11, 0),
                        stop_time: time_hm(18, 0),
                    },
                    Blank,
                ],
            },
            Day {
                date: NaiveDate::from_ymd_opt(2020, 7, 16).unwrap(),
                lines: vec![
                    ClosedShift {
                        start_time: time_hm(8, 0),
                        stop_time: time_hm(12, 0),
                    },
                    SpecialShift {
                        text: String::from("VAB"),
                        start_time: time_hm(13, 0),
                        stop_time: time_hm(17, 0),
                    },
                    Blank,
                ],
            },
            Day {
                date: NaiveDate::from_ymd_opt(2020, 7, 17).unwrap(),
                lines: vec![OpenShift {
                    start_time: time_hm(8, 12),
                }],
            },
        ],
    }
}

fn example_1_week() -> IsoWeek {
    NaiveDate::from_ymd_opt(2020, 7, 3).unwrap().iso_week()
}

fn example_1_text() -> String {
    String::from(
        "# Preamble
* carry 1h 10m
* negativecarry -2h -10m

[monday 2020-07-13]
* Vacation
# Came back from Jämtland

[tuesday 2020-07-14]
* 08:32-12:02
* 12:30-13:01
* 13:45-18:03

[wednesday 2020-07-15]
* 11:00-18:00

[thursday 2020-07-16]
* 08:00-12:00
* VAB 13:00-17:00

[friday 2020-07-17]
* 08:12-
",
    )
}
