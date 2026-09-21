use chrono::Duration;

use crate::config::WorkWeekConfig;
use crate::document::{Day, Document, Line};
use crate::report::Report;
use crate::report::render::{summary, week};
use crate::testutils::{iso_week, naive_date, naive_date_time, naive_time};

fn closed(from: (u32, u32), to: (u32, u32)) -> Line {
    Line::ClosedShift {
        start_time: naive_time(from.0, from.1),
        stop_time: naive_time(to.0, to.1),
    }
}

fn report(duration_today: Duration, duration_week: Duration, balance: Duration) -> Report {
    Report {
        duration_today,
        duration_week,
        is_ongoing: false,
        balance,
    }
}

#[test]
fn current_week_summary_is_in_the_present_tense() {
    let report = report(
        Duration::hours(3),
        Duration::hours(11),
        Duration::minutes(-40),
    );
    assert_eq!(
        "You have worked 3h today.\n\
         You have worked 11h this week.\n\
         Balance: -40m\n",
        summary(&report, true)
    )
}

#[test]
fn an_ongoing_shift_is_said_to_be_ongoing() {
    let report = Report {
        is_ongoing: true,
        ..report(Duration::hours(3), Duration::hours(11), Duration::zero())
    };
    assert!(summary(&report, true).starts_with("You have worked 3h today, ongoing.\n"))
}

#[test]
fn an_earlier_week_summary_is_in_the_past_tense_and_has_no_today() {
    let report = report(
        Duration::zero(),
        Duration::minutes(2121),
        Duration::minutes(40),
    );
    assert_eq!(
        "You worked 35:21h this week.\n\
         Balance: +40m\n",
        summary(&report, false)
    )
}

#[test]
fn listing_annotates_every_shift_and_every_day() {
    let document = Document::new(
        iso_week(2026, 38),
        vec![
            Line::Comment {
                text: String::from("balance carried over from 2026-W37"),
            },
            Line::DurationShift {
                text: String::from("balance"),
                duration: Duration::minutes(319),
            },
            Line::Blank,
        ],
        vec![
            Day {
                date: naive_date(2026, 9, 14),
                lines: vec![
                    closed((8, 16), (8, 40)),
                    closed((9, 57), (17, 0)),
                    Line::Blank,
                ],
            },
            Day {
                date: naive_date(2026, 9, 15),
                lines: vec![Line::SpecialDay {
                    text: String::from("vacation"),
                }],
            },
        ],
    );
    // A day in a later week, so nothing here counts as today.
    let now = naive_date_time(2026, 9, 21, 9, 0);

    assert_eq!(
        "\
# balance carried over from 2026-W37
* balance 5:19h

[monday 2026-09-14]   7:27h
* 08:16-08:40           24m
* 09:57-17:00         7:03h

[tuesday 2026-09-15]     8h
* vacation               8h
",
        week(&document, &now, &WorkWeekConfig::default(), false)
    )
}

#[test]
fn todays_open_shift_counts_up_to_now() {
    let document = Document::new(
        iso_week(2026, 39),
        vec![],
        vec![Day {
            date: naive_date(2026, 9, 21),
            lines: vec![
                closed((6, 21), (7, 0)),
                Line::OpenShift {
                    start_time: naive_time(8, 30),
                },
            ],
        }],
    );
    let now = naive_date_time(2026, 9, 21, 9, 0);

    assert_eq!(
        "\
[monday 2026-09-21]  1:09h
* 06:21-07:00          39m
* 08:30-               30m
",
        week(&document, &now, &WorkWeekConfig::default(), false)
    )
}

#[test]
fn an_open_shift_left_behind_on_an_earlier_day_is_called_out() {
    let document = Document::new(
        iso_week(2026, 39),
        vec![],
        vec![Day {
            date: naive_date(2026, 9, 21),
            lines: vec![Line::OpenShift {
                start_time: naive_time(8, 30),
            }],
        }],
    );
    let now = naive_date_time(2026, 9, 22, 9, 0);

    assert_eq!(
        "\
[monday 2026-09-21]          0h
* 08:30-             not closed
",
        week(&document, &now, &WorkWeekConfig::default(), false)
    )
}

#[test]
fn colours_are_only_written_when_asked_for() {
    let document = Document::new(
        iso_week(2026, 38),
        vec![],
        vec![Day {
            date: naive_date(2026, 9, 14),
            lines: vec![closed((8, 0), (12, 0))],
        }],
    );
    let now = naive_date_time(2026, 9, 21, 9, 0);

    let colored = week(&document, &now, &WorkWeekConfig::default(), true);
    assert!(colored.contains('\u{1b}'));
    assert_eq!(
        week(&document, &now, &WorkWeekConfig::default(), false),
        console::strip_ansi_codes(&colored)
    )
}
