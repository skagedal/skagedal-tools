//! Rendering a selection of journeys, as a table or as JSON.
//!
//! Everything here is a pure function of the report, so the layout is
//! testable without a network or a terminal.

use chrono::{DateTime, FixedOffset};
use serde_json::json;

use crate::journeys::{Hidden, Journey, Selection};
use crate::ticket::{Coverage, Ticket};

/// One end of the route, as resolved against the API's station list.
pub struct Endpoint<'a> {
    pub signature: &'a str,
    pub name: &'a str,
}

pub struct Report<'a> {
    pub from: Endpoint<'a>,
    pub to: Endpoint<'a>,
    pub now: DateTime<FixedOffset>,
    pub window_minutes: i64,
    pub ticket: &'a Ticket,
    pub selection: &'a Selection,
}

/// Colour, applied only when the caller says the output is going to a
/// terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ink {
    None,
    Dim,
    Bold,
    Red,
    Yellow,
}

const INDENT: &str = "  ";
const GAP: &str = "  ";

pub fn render(report: &Report, color: bool) -> String {
    let mut out = String::new();
    out.push_str(&header(report, color));
    out.push('\n');

    if report.selection.journeys.is_empty() {
        out.push('\n');
        out.push_str(&paint(&empty_message(report), Ink::Dim, color));
        out.push('\n');
    } else {
        out.push('\n');
        out.push_str(&table(report, color));
    }

    if let Some(note) = hidden_note(&report.selection.hidden) {
        out.push('\n');
        out.push_str(&paint(&note, Ink::Dim, color));
        out.push('\n');
    }
    out
}

fn header(report: &Report, color: bool) -> String {
    let route = format!("{} → {}", report.from.name, report.to.name);
    let mut line = paint(&route, Ink::Bold, color);
    let when = report.now.format("%a %-d %b %H:%M").to_string();
    line.push_str(&paint(&format!(" · {when}"), Ink::Dim, color));
    if let Some(products) = report.ticket.products.as_ref()
        && !products.is_empty()
    {
        line.push_str(&paint(
            &format!(" · {}", products.join(", ")),
            Ink::Dim,
            color,
        ));
    }
    line
}

fn empty_message(report: &Report) -> String {
    let window = format_window(report.window_minutes);
    if report.ticket.is_unrestricted() {
        format!("No departures to {} in the next {window}.", report.to.name)
    } else {
        format!(
            "No departures to {} in the next {window} that this ticket covers.",
            report.to.name
        )
    }
}

fn hidden_note(hidden: &Hidden) -> Option<String> {
    let mut parts = Vec::new();
    if hidden.uncovered > 0 {
        parts.push(format!("{} not covered", hidden.uncovered));
    }
    if hidden.canceled > 0 {
        parts.push(format!("{} cancelled", hidden.canceled));
    }
    if parts.is_empty() {
        return None;
    }
    let total: usize = hidden.uncovered + hidden.canceled;
    let departures = if total == 1 {
        "departure"
    } else {
        "departures"
    };
    Some(format!(
        "{total} {departures} hidden ({}) — pass --all to see them.",
        parts.join(", ")
    ))
}

fn table(report: &Report, color: bool) -> String {
    let rows: Vec<Row> = report
        .selection
        .journeys
        .iter()
        .map(|j| row(j, report.now))
        .collect();

    let columns = rows.first().map(|r| r.cells.len()).unwrap_or(0);
    let widths: Vec<usize> = (0..columns)
        .map(|i| {
            rows.iter()
                .map(|r| width(&r.cells[i].text))
                .max()
                .unwrap_or(0)
        })
        .collect();

    let mut out = String::new();
    for row in &rows {
        let mut line = String::from(INDENT);
        for (i, cell) in row.cells.iter().enumerate() {
            if i > 0 {
                line.push_str(GAP);
            }
            let ink = if row.dim { Ink::Dim } else { cell.ink };
            let padding = " ".repeat(widths[i].saturating_sub(width(&cell.text)));
            if i == 0 {
                line.push_str(&padding);
                line.push_str(&paint(&cell.text, ink, color));
            } else {
                line.push_str(&paint(&cell.text, ink, color));
                line.push_str(&padding);
            }
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

struct Cell {
    text: String,
    ink: Ink,
}

struct Row {
    cells: Vec<Cell>,
    dim: bool,
}

fn cell(text: impl Into<String>, ink: Ink) -> Cell {
    Cell {
        text: text.into(),
        ink,
    }
}

fn row(journey: &Journey, now: DateTime<FixedOffset>) -> Row {
    let departure = journey.departure.expected();
    let times = format!(
        "{} → {}",
        departure.format("%H:%M"),
        journey.arrival.expected().format("%H:%M")
    );
    let track = match journey.departure.track.as_deref() {
        Some(track) => format!("track {track}"),
        None => "–".to_string(),
    };
    let notes = notes(journey);
    let notes_ink = if journey.is_canceled() || journey.departure.delay_minutes() > 0 {
        Ink::Red
    } else if !notes.is_empty() {
        Ink::Yellow
    } else {
        Ink::None
    };
    Row {
        cells: vec![
            cell(format_relative(now, departure), Ink::None),
            cell(times, Ink::None),
            cell(train_name(journey), Ink::None),
            cell(track, Ink::Dim),
            cell(notes, notes_ink),
        ],
        dim: journey.is_canceled() || !journey.coverage.is_covered(),
    }
}

fn train_name(journey: &Journey) -> String {
    match journey.products.first() {
        Some(product) => format!("{product} {}", journey.train),
        None => format!("train {}", journey.train),
    }
}

fn notes(journey: &Journey) -> String {
    let mut parts: Vec<String> = Vec::new();
    if journey.is_canceled() {
        parts.push("cancelled".to_string());
    }
    let departure_delay = journey.departure.delay_minutes();
    if departure_delay > 0 {
        parts.push(format!(
            "{departure_delay} min late (timetabled {})",
            journey.departure.advertised.format("%H:%M")
        ));
    } else if departure_delay < 0 {
        parts.push(format!(
            "{} min early (timetabled {})",
            -departure_delay,
            journey.departure.advertised.format("%H:%M")
        ));
    } else if journey.arrival.delay_minutes() > 0 {
        parts.push(format!(
            "arrives {} min late",
            journey.arrival.delay_minutes()
        ));
    }
    match journey.coverage {
        Coverage::Covered => {}
        Coverage::NotCovered => parts.push("not covered by this ticket".to_string()),
        Coverage::Unknown => parts.push("no product information".to_string()),
    }
    for deviation in &journey.deviations {
        if !parts.iter().any(|p| p.eq_ignore_ascii_case(deviation)) {
            parts.push(deviation.clone());
        }
    }
    parts.join(", ")
}

/// "now", "in 7 min", "in 1 h 34 min".
fn format_relative(now: DateTime<FixedOffset>, then: DateTime<FixedOffset>) -> String {
    let minutes = (then - now).num_minutes();
    if minutes <= 0 {
        return "now".to_string();
    }
    if minutes < 60 {
        return format!("in {minutes} min");
    }
    let (hours, rest) = (minutes / 60, minutes % 60);
    if rest == 0 {
        format!("in {hours} h")
    } else {
        format!("in {hours} h {rest} min")
    }
}

/// "45 min", "3 h", "1 h 30 min".
fn format_window(minutes: i64) -> String {
    if minutes < 60 {
        return format!("{minutes} min");
    }
    let (hours, rest) = (minutes / 60, minutes % 60);
    if rest == 0 {
        format!("{hours} h")
    } else {
        format!("{hours} h {rest} min")
    }
}

fn width(text: &str) -> usize {
    console::measure_text_width(text)
}

fn paint(text: &str, ink: Ink, color: bool) -> String {
    if !color || ink == Ink::None || text.is_empty() {
        return text.to_string();
    }
    let styled = console::style(text).force_styling(true);
    match ink {
        Ink::Dim => styled.dim(),
        Ink::Bold => styled.bold(),
        Ink::Red => styled.red(),
        Ink::Yellow => styled.yellow(),
        Ink::None => styled,
    }
    .to_string()
}

/// The same report as JSON, for scripts and for wiring into other tools.
pub fn render_json(report: &Report) -> serde_json::Value {
    json!({
        "from": {"signature": report.from.signature, "name": report.from.name},
        "to": {"signature": report.to.signature, "name": report.to.name},
        "now": report.now.to_rfc3339(),
        "windowMinutes": report.window_minutes,
        "products": report.ticket.products,
        "journeys": report.selection.journeys,
        "hidden": {
            "departed": report.selection.hidden.departed,
            "canceled": report.selection.hidden.canceled,
            "uncovered": report.selection.hidden.uncovered,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journeys::Stop;

    fn time(s: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(s).unwrap()
    }

    fn stop(advertised: &str, estimated: Option<&str>, track: Option<&str>) -> Stop {
        Stop {
            advertised: time(advertised),
            estimated: estimated.map(time),
            actual: None,
            track: track.map(str::to_owned),
            canceled: false,
        }
    }

    fn journey(train: &str, product: &str, departure: Stop, arrival: Stop) -> Journey {
        Journey {
            train: train.to_string(),
            products: vec![product.to_string()],
            destination: Some("Lp".to_string()),
            operator: Some("Mälardalstrafik".to_string()),
            departure,
            arrival,
            deviations: vec![],
            coverage: Coverage::Covered,
        }
    }

    fn report<'a>(selection: &'a Selection, ticket: &'a Ticket) -> Report<'a> {
        Report {
            from: Endpoint {
                signature: "U",
                name: "Uppsala C",
            },
            to: Endpoint {
                signature: "Cst",
                name: "Stockholm C",
            },
            now: time("2026-09-10T09:10:00+02:00"),
            window_minutes: 180,
            ticket,
            selection,
        }
    }

    fn selection(journeys: Vec<Journey>, hidden: Hidden) -> Selection {
        Selection { journeys, hidden }
    }

    #[test]
    fn renders_a_table() {
        let js = vec![
            journey(
                "2137",
                "Mälartåg",
                stop("2026-09-10T09:12:00+02:00", None, Some("3")),
                stop("2026-09-10T09:51:00+02:00", None, None),
            ),
            journey(
                "634",
                "SJ Regional",
                stop(
                    "2026-09-10T09:35:00+02:00",
                    Some("2026-09-10T09:39:00+02:00"),
                    Some("9"),
                ),
                stop(
                    "2026-09-10T10:14:00+02:00",
                    Some("2026-09-10T10:18:00+02:00"),
                    None,
                ),
            ),
        ];
        let ticket = Ticket::for_products(["Mälartåg", "SJ Regional"]);
        let sel = selection(js, Hidden::default());
        let text = render(&report(&sel, &ticket), false);
        assert_eq!(
            text,
            "Uppsala C → Stockholm C · Thu 10 Sep 09:10 · Mälartåg, SJ Regional\n\
             \n\
             \x20\x20 in 2 min  09:12 → 09:51  Mälartåg 2137    track 3\n\
             \x20\x20in 29 min  09:39 → 10:18  SJ Regional 634  track 9  4 min late (timetabled 09:35)\n"
        );
    }

    #[test]
    fn marks_a_train_the_ticket_does_not_cover() {
        let mut j = journey(
            "424",
            "SJ Snabbtåg",
            stop("2026-09-10T09:31:00+02:00", None, Some("6")),
            stop("2026-09-10T09:49:00+02:00", None, None),
        );
        j.coverage = Coverage::NotCovered;
        let ticket = Ticket::for_products(["Mälartåg"]);
        let sel = selection(vec![j], Hidden::default());
        let text = render(&report(&sel, &ticket), false);
        assert!(text.contains("not covered by this ticket"), "{text}");
    }

    #[test]
    fn marks_a_cancelled_train_and_has_no_track_to_show() {
        let mut j = journey(
            "2139",
            "Mälartåg",
            stop("2026-09-10T09:42:00+02:00", None, None),
            stop("2026-09-10T10:25:00+02:00", None, None),
        );
        j.departure.canceled = true;
        let ticket = Ticket::for_products(["Mälartåg"]);
        let sel = selection(vec![j], Hidden::default());
        let text = render(&report(&sel, &ticket), false);
        assert!(text.contains("cancelled"), "{text}");
        assert!(text.contains(" – "), "{text}");
    }

    #[test]
    fn reports_an_arrival_delay_when_the_departure_is_on_time() {
        let j = journey(
            "2137",
            "Mälartåg",
            stop("2026-09-10T09:12:00+02:00", None, Some("3")),
            stop(
                "2026-09-10T09:51:00+02:00",
                Some("2026-09-10T09:58:00+02:00"),
                None,
            ),
        );
        let ticket = Ticket::unrestricted();
        let sel = selection(vec![j], Hidden::default());
        let text = render(&report(&sel, &ticket), false);
        assert!(text.contains("arrives 7 min late"), "{text}");
    }

    #[test]
    fn says_when_nothing_was_found() {
        let ticket = Ticket::for_products(["Mälartåg"]);
        let sel = selection(vec![], Hidden::default());
        let text = render(&report(&sel, &ticket), false);
        assert!(
            text.contains("No departures to Stockholm C in the next 3 h that this ticket covers."),
            "{text}"
        );
    }

    #[test]
    fn counts_what_was_hidden() {
        let ticket = Ticket::for_products(["Mälartåg"]);
        let sel = selection(
            vec![],
            Hidden {
                departed: 4,
                canceled: 1,
                uncovered: 2,
            },
        );
        let text = render(&report(&sel, &ticket), false);
        assert!(
            text.contains(
                "3 departures hidden (2 not covered, 1 cancelled) — pass --all to see them."
            ),
            "{text}"
        );
        // Trains that have already left are not worth mentioning.
        assert!(!text.contains('4'), "{text}");
    }

    #[test]
    fn nothing_hidden_means_no_note() {
        let ticket = Ticket::unrestricted();
        let sel = selection(
            vec![],
            Hidden {
                departed: 2,
                ..Default::default()
            },
        );
        let text = render(&report(&sel, &ticket), false);
        assert!(!text.contains("hidden"), "{text}");
    }

    #[test]
    fn colour_is_only_applied_when_asked_for() {
        let j = journey(
            "2137",
            "Mälartåg",
            stop("2026-09-10T09:12:00+02:00", None, Some("3")),
            stop("2026-09-10T09:51:00+02:00", None, None),
        );
        let ticket = Ticket::unrestricted();
        let sel = selection(vec![j], Hidden::default());
        assert!(!render(&report(&sel, &ticket), false).contains('\x1b'));
        assert!(render(&report(&sel, &ticket), true).contains('\x1b'));
    }

    #[test]
    fn formats_relative_times() {
        let now = time("2026-09-10T09:10:00+02:00");
        assert_eq!(
            format_relative(now, time("2026-09-10T09:10:30+02:00")),
            "now"
        );
        assert_eq!(
            format_relative(now, time("2026-09-10T09:00:00+02:00")),
            "now"
        );
        assert_eq!(
            format_relative(now, time("2026-09-10T09:17:00+02:00")),
            "in 7 min"
        );
        assert_eq!(
            format_relative(now, time("2026-09-10T10:10:00+02:00")),
            "in 1 h"
        );
        assert_eq!(
            format_relative(now, time("2026-09-10T10:44:00+02:00")),
            "in 1 h 34 min"
        );
    }

    #[test]
    fn formats_windows() {
        assert_eq!(format_window(45), "45 min");
        assert_eq!(format_window(180), "3 h");
        assert_eq!(format_window(90), "1 h 30 min");
    }

    #[test]
    fn json_carries_the_route_and_the_journeys() {
        let j = journey(
            "2137",
            "Mälartåg",
            stop("2026-09-10T09:12:00+02:00", None, Some("3")),
            stop("2026-09-10T09:51:00+02:00", None, None),
        );
        let ticket = Ticket::for_products(["Mälartåg"]);
        let sel = selection(vec![j], Hidden::default());
        let value = render_json(&report(&sel, &ticket));
        assert_eq!(value["from"]["signature"], "U");
        assert_eq!(value["to"]["name"], "Stockholm C");
        assert_eq!(value["journeys"][0]["train"], "2137");
        assert_eq!(value["journeys"][0]["coverage"], "covered");
        assert_eq!(value["journeys"][0]["departure"]["track"], "3");
        assert_eq!(value["products"][0], "Mälartåg");
    }
}
