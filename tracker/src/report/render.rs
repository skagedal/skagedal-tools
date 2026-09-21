//! Turning a report into the text that `tracker report` prints.
//!
//! Everything here is a pure function of the document and the report, so the
//! wording and the layout are testable without a terminal.

use chrono::{Duration, NaiveDateTime};

use crate::config::WorkWeekConfig;
use crate::document::{Day, Document, Line};
use crate::duration::{format_duration, format_signed_duration};

use super::{Report, duration_for_day, duration_for_line, duration_for_today};

/// Colour, applied only when the caller says the output is going to a
/// terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ink {
    None,
    Dim,
    Cyan,
    Yellow,
}

/// Spaces between the widest annotated line and the annotation column.
const GAP: usize = 2;

/// One line of output: the file text, and what is written beside it.
struct Row {
    text: String,
    annotation: String,
    ink: Ink,
}

fn plain(text: String) -> Row {
    Row {
        text,
        annotation: String::new(),
        ink: Ink::None,
    }
}

/// The three lines of the normal report. A week that is not the current one is
/// spoken of in the past tense, and has no "today" in it to report on.
pub fn summary(report: &Report, is_current_week: bool) -> String {
    let mut out = String::new();
    if is_current_week {
        out.push_str(&format!(
            "You have worked {} today{}\n",
            format_duration(report.duration_today),
            if report.is_ongoing { ", ongoing." } else { "." }
        ));
        out.push_str(&format!(
            "You have worked {} this week.\n",
            format_duration(report.duration_week)
        ));
    } else {
        out.push_str(&format!(
            "You worked {} this week.\n",
            format_duration(report.duration_week)
        ));
    }
    out.push_str(&format!(
        "Balance: {}\n",
        format_signed_duration(report.balance)
    ));
    out
}

/// The whole week file as `edit` would show it, with the length of every shift
/// and the sum of every day annotated beside it.
pub fn week(
    document: &Document,
    now: &NaiveDateTime,
    workweek: &WorkWeekConfig,
    color: bool,
) -> String {
    let mut rows: Vec<Row> = document
        .preamble
        .iter()
        .map(|line| plain(line.to_string()))
        .collect();
    for day in &document.days {
        let is_today = day.date == now.date();
        rows.push(Row {
            text: day.header_line(),
            annotation: format_duration(day_total(day, now, workweek, is_today)),
            ink: Ink::Cyan,
        });
        rows.extend(
            day.lines
                .iter()
                .map(|line| shift_row(line, now, workweek, is_today)),
        );
    }
    layout(&rows, color)
}

/// Lines without an annotation – comments, blanks – are left out of the column
/// width, so a long comment doesn't push every annotation to the right.
fn layout(rows: &[Row], color: bool) -> String {
    let annotated = || rows.iter().filter(|row| !row.annotation.is_empty());
    let text_width = annotated().map(|row| width(&row.text)).max().unwrap_or(0);
    let annotation_width = annotated()
        .map(|row| width(&row.annotation))
        .max()
        .unwrap_or(0);

    let mut out = String::new();
    for row in rows {
        if row.annotation.is_empty() {
            out.push_str(&row.text);
        } else {
            let padding = text_width - width(&row.text) + GAP;
            out.push_str(&row.text);
            out.push_str(&" ".repeat(padding));
            out.push_str(&paint(
                &format!("{:>annotation_width$}", row.annotation),
                row.ink,
                color,
            ));
        }
        out.push('\n');
    }
    out
}

fn day_total(
    day: &Day,
    now: &NaiveDateTime,
    workweek: &WorkWeekConfig,
    is_today: bool,
) -> Duration {
    match is_today {
        true => duration_for_today(day, now, workweek),
        false => duration_for_day(day, workweek),
    }
}

/// An open shift counts towards the day only while it is today's; one left
/// behind on an earlier day counts as nothing, and is called out as the
/// mistake it is rather than annotated with a duration.
fn shift_row(line: &Line, now: &NaiveDateTime, workweek: &WorkWeekConfig, is_today: bool) -> Row {
    let text = line.to_string();
    match line {
        Line::ClosedShift { .. } | Line::SpecialShift { .. } | Line::SpecialDay { .. } => Row {
            annotation: format_duration(duration_for_line(line, None, workweek)),
            ink: Ink::Dim,
            text,
        },
        Line::OpenShift { .. } if is_today => Row {
            annotation: format_duration(duration_for_line(line, Some(*now), workweek)),
            ink: Ink::Dim,
            text,
        },
        Line::OpenShift { .. } => Row {
            annotation: String::from("not closed"),
            ink: Ink::Yellow,
            text,
        },
        _ => plain(text),
    }
}

fn width(text: &str) -> usize {
    console::measure_text_width(text)
}

fn paint(text: &str, ink: Ink, color: bool) -> String {
    if !color || ink == Ink::None {
        return text.to_string();
    }
    let styled = console::style(text).force_styling(true);
    match ink {
        Ink::Dim => styled.dim(),
        Ink::Cyan => styled.cyan(),
        Ink::Yellow => styled.yellow(),
        Ink::None => styled,
    }
    .to_string()
}

#[cfg(test)]
mod tests;
