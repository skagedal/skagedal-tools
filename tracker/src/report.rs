use std::ops::{Add, Sub};

use crate::config::WorkWeekConfig;
use crate::document::{Day, Document, Line};
use chrono::{Duration, IsoWeek, NaiveDate, NaiveDateTime, TimeDelta};

#[derive(PartialEq, Debug, Clone)]
pub struct Report {
    pub duration_today: Duration,
    pub duration_week: Duration,
    pub is_ongoing: bool,
    pub balance: Duration,
}

fn duration_for_line(
    line: &Line,
    now: Option<NaiveDateTime>,
    workweek: &WorkWeekConfig,
) -> Duration {
    match line {
        Line::ClosedShift {
            start_time,
            stop_time,
        } => stop_time.signed_duration_since(*start_time),
        Line::OpenShift { start_time } => now
            .map(|now| now.time().signed_duration_since(*start_time))
            .unwrap_or_else(Duration::zero),
        Line::SpecialShift {
            start_time,
            stop_time,
            ..
        } => stop_time.signed_duration_since(*start_time),
        Line::SpecialDay { .. } => TimeDelta::try_hours(workweek.hours_per_day.into()).unwrap(),
        _ => Duration::zero(),
    }
}

fn duration_for_day(day: &Day, workweek: &WorkWeekConfig) -> Duration {
    day.lines.iter().fold(TimeDelta::zero(), |acc, line| {
        acc + duration_for_line(line, None, workweek)
    })
}

fn duration_for_today(day: &Day, now: &NaiveDateTime, workweek: &WorkWeekConfig) -> Duration {
    day.lines.iter().fold(TimeDelta::zero(), |acc, line| {
        acc + duration_for_line(line, Some(*now), workweek)
    })
}

fn expected_days_worked(week: IsoWeek, now: &NaiveDateTime, workweek: &WorkWeekConfig) -> u32 {
    let now = now.date();
    let monday_of_week =
        NaiveDate::from_isoywd_opt(week.year(), week.week(), chrono::Weekday::Mon).unwrap();
    let days_since = now.signed_duration_since(monday_of_week).num_days() + 1;
    num_traits::clamp(days_since as u32, 0, workweek.days_per_week)
}

impl Report {
    pub fn from_document(
        document: &Document,
        now: &NaiveDateTime,
        workweek: &WorkWeekConfig,
    ) -> Report {
        let this_day = document.days.iter().find(|day| day.date == now.date());
        let duration_today = this_day
            .map(|day| duration_for_today(day, now, workweek))
            .unwrap_or_else(Duration::zero);
        let duration_week = document
            .days
            .iter()
            .filter(|day| day.date != now.date())
            .fold(TimeDelta::zero(), |acc, day| {
                acc + duration_for_day(day, workweek)
            })
            .add(duration_today);

        let expected_days_so_far = expected_days_worked(document.week, now, workweek);
        let expected_duration_so_far_week =
            TimeDelta::try_hours((expected_days_so_far * workweek.hours_per_day).into()).unwrap();
        let incoming_balance: Duration = document
            .preamble
            .iter()
            .filter_map(|d| match d {
                Line::DurationShift { text: _, duration } => Some(duration),
                _ => None,
            })
            .sum();

        Report {
            duration_today,
            duration_week,
            is_ongoing: this_day.map(Day::has_open_shift).unwrap_or_else(|| false),
            balance: duration_week
                .sub(expected_duration_so_far_week)
                .add(incoming_balance),
        }
    }
}

#[cfg(test)]
mod report_tests;
