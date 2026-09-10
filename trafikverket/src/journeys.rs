//! Turning two lists of station announcements into journeys between them.
//!
//! `TrainAnnouncement` is one row per train per station, so a trip is
//! assembled by joining the departures at the origin against the arrivals at
//! the destination on the train number and its departure date. That join is
//! also what filters by direction: a train leaving Uppsala northbound never
//! shows up among the arrivals at Stockholm C, so it drops out on its own,
//! and the ones that survive come with an arrival time attached.

use std::collections::HashMap;

use chrono::{DateTime, FixedOffset};
use serde::Serialize;

use crate::model::TrainAnnouncement;
use crate::ticket::{Coverage, Ticket};

/// One train calling at one station.
#[derive(Debug, Clone, Serialize)]
pub struct Stop {
    pub advertised: DateTime<FixedOffset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated: Option<DateTime<FixedOffset>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<DateTime<FixedOffset>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<String>,
    pub canceled: bool,
}

impl Stop {
    fn from_announcement(a: &TrainAnnouncement) -> Option<Stop> {
        Some(Stop {
            advertised: a.advertised?,
            estimated: a.estimated,
            actual: a.actual,
            track: a
                .track
                .as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_owned),
            canceled: a.canceled,
        })
    }

    /// When the train is actually expected: what happened, else the current
    /// forecast, else the timetable.
    pub fn expected(&self) -> DateTime<FixedOffset> {
        self.actual.or(self.estimated).unwrap_or(self.advertised)
    }

    /// Minutes late against the timetable. Negative when running early.
    pub fn delay_minutes(&self) -> i64 {
        (self.expected() - self.advertised).num_minutes()
    }

    /// Whether the train has already left this station.
    pub fn has_gone(&self, now: DateTime<FixedOffset>) -> bool {
        self.actual.is_some() || self.expected() < now
    }
}

/// A direct trip from the origin station to the destination station.
#[derive(Debug, Clone, Serialize)]
pub struct Journey {
    pub train: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub products: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator: Option<String>,
    pub departure: Stop,
    pub arrival: Stop,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub deviations: Vec<String>,
    pub coverage: Coverage,
}

impl Journey {
    pub fn is_canceled(&self) -> bool {
        self.departure.canceled || self.arrival.canceled
    }
}

/// Join departures at the origin with arrivals at the destination.
///
/// Result is ordered by when the train is expected to leave.
pub fn build(
    departures: &[TrainAnnouncement],
    arrivals: &[TrainAnnouncement],
    ticket: &Ticket,
) -> Vec<Journey> {
    let mut by_train: HashMap<&str, Vec<&TrainAnnouncement>> = HashMap::new();
    for arrival in arrivals {
        if let Some(ident) = arrival.train_ident.as_deref() {
            by_train.entry(ident).or_default().push(arrival);
        }
    }

    let mut journeys: Vec<Journey> = Vec::new();
    for departure in departures {
        let Some(ident) = departure.train_ident.as_deref() else {
            continue;
        };
        let Some(dep_stop) = Stop::from_announcement(departure) else {
            continue;
        };
        let Some(arrival) = by_train.get(ident).and_then(|candidates| {
            candidates
                .iter()
                .filter(|a| same_run(departure, a))
                .filter_map(|a| {
                    let stop = Stop::from_announcement(a)?;
                    // The timetable, not the forecast, decides which
                    // announcement is the far end of this trip: a delay
                    // reported at one station but not yet the other must not
                    // make the arrival look earlier than the departure.
                    (stop.advertised > dep_stop.advertised).then_some((a, stop))
                })
                .min_by_key(|(_, stop)| stop.advertised)
        }) else {
            continue;
        };
        let (arrival_announcement, arrival_stop) = arrival;

        let products = departure.products();
        let coverage = ticket.coverage(&products);
        let mut deviations = departure.deviations();
        for d in arrival_announcement.deviations() {
            if !deviations.contains(&d) {
                deviations.push(d);
            }
        }

        journeys.push(Journey {
            train: ident.to_string(),
            products,
            destination: departure.destination().map(str::to_owned),
            operator: departure.information_owner.clone(),
            departure: dep_stop,
            arrival: arrival_stop,
            deviations,
            coverage,
        });
    }

    journeys.sort_by_key(|j| (j.departure.expected(), j.departure.advertised));
    journeys
}

/// Two announcements belong to the same run when the train numbers match and
/// the scheduled departure dates do not contradict each other. The date is
/// what keeps yesterday's delayed 2137 apart from today's.
fn same_run(departure: &TrainAnnouncement, arrival: &TrainAnnouncement) -> bool {
    match (
        departure.scheduled_departure_date.as_deref(),
        arrival.scheduled_departure_date.as_deref(),
    ) {
        (Some(a), Some(b)) => a == b,
        _ => true,
    }
}

/// What was left out of a selection, so the caller can say so.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Hidden {
    pub departed: usize,
    pub canceled: usize,
    pub uncovered: usize,
}

#[derive(Debug, Clone)]
pub struct Selection {
    pub journeys: Vec<Journey>,
    pub hidden: Hidden,
}

/// Pick the journeys worth showing: the ones still catchable, and — unless
/// `include_all` — only those the ticket covers and that are not cancelled.
pub fn select(
    journeys: Vec<Journey>,
    now: DateTime<FixedOffset>,
    limit: usize,
    include_all: bool,
) -> Selection {
    let mut hidden = Hidden::default();
    let mut kept = Vec::new();
    for journey in journeys {
        if journey.departure.has_gone(now) {
            hidden.departed += 1;
            continue;
        }
        if journey.is_canceled() && !include_all {
            hidden.canceled += 1;
            continue;
        }
        if !journey.coverage.is_covered() && !include_all {
            hidden.uncovered += 1;
            continue;
        }
        kept.push(journey);
    }
    kept.truncate(limit);
    Selection {
        journeys: kept,
        hidden,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CodedText;

    fn time(s: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(s).unwrap()
    }

    struct Announcement(TrainAnnouncement);

    fn announcement(ident: &str, advertised: &str) -> Announcement {
        Announcement(TrainAnnouncement {
            train_ident: Some(ident.to_string()),
            scheduled_departure_date: Some("2026-09-10T00:00:00.000+02:00".to_string()),
            advertised: Some(time(advertised)),
            ..Default::default()
        })
    }

    impl Announcement {
        fn product(mut self, name: &str) -> Self {
            self.0
                .product_information
                .push(CodedText::Text(name.to_string()));
            self
        }
        fn estimated(mut self, at: &str) -> Self {
            self.0.estimated = Some(time(at));
            self
        }
        fn actual(mut self, at: &str) -> Self {
            self.0.actual = Some(time(at));
            self
        }
        fn track(mut self, track: &str) -> Self {
            self.0.track = Some(track.to_string());
            self
        }
        fn canceled(mut self) -> Self {
            self.0.canceled = true;
            self
        }
        fn date(mut self, date: &str) -> Self {
            self.0.scheduled_departure_date = Some(date.to_string());
            self
        }
        fn build(self) -> TrainAnnouncement {
            self.0
        }
    }

    fn movingo() -> Ticket {
        Ticket::for_products(["Mälartåg", "SJ Regional"])
    }

    #[test]
    fn joins_a_departure_to_its_arrival() {
        let deps = vec![
            announcement("2137", "2026-09-10T09:12:00+02:00")
                .product("Mälartåg")
                .track("3")
                .build(),
        ];
        let arrs = vec![announcement("2137", "2026-09-10T09:51:00+02:00").build()];
        let journeys = build(&deps, &arrs, &movingo());
        assert_eq!(journeys.len(), 1);
        let j = &journeys[0];
        assert_eq!(j.train, "2137");
        assert_eq!(j.departure.track.as_deref(), Some("3"));
        assert_eq!(j.arrival.advertised, time("2026-09-10T09:51:00+02:00"));
        assert_eq!(j.coverage, Coverage::Covered);
    }

    #[test]
    fn drops_departures_that_never_reach_the_destination() {
        let deps = vec![
            announcement("2137", "2026-09-10T09:12:00+02:00").build(),
            announcement("8801", "2026-09-10T09:20:00+02:00").build(),
        ];
        let arrs = vec![announcement("2137", "2026-09-10T09:51:00+02:00").build()];
        let journeys = build(&deps, &arrs, &Ticket::unrestricted());
        assert_eq!(journeys.len(), 1);
        assert_eq!(journeys[0].train, "2137");
    }

    #[test]
    fn does_not_join_an_arrival_that_precedes_the_departure() {
        let deps = vec![announcement("2137", "2026-09-10T09:12:00+02:00").build()];
        let arrs = vec![announcement("2137", "2026-09-10T08:30:00+02:00").build()];
        assert!(build(&deps, &arrs, &Ticket::unrestricted()).is_empty());
    }

    #[test]
    fn does_not_join_across_scheduled_departure_dates() {
        let deps = vec![
            announcement("2137", "2026-09-10T23:45:00+02:00")
                .date("2026-09-10T00:00:00.000+02:00")
                .build(),
        ];
        let arrs = vec![
            announcement("2137", "2026-09-11T00:25:00+02:00")
                .date("2026-09-11T00:00:00.000+02:00")
                .build(),
        ];
        assert!(build(&deps, &arrs, &Ticket::unrestricted()).is_empty());
    }

    #[test]
    fn picks_the_earliest_arrival_after_the_departure() {
        let deps = vec![announcement("2137", "2026-09-10T09:12:00+02:00").build()];
        let arrs = vec![
            announcement("2137", "2026-09-10T11:30:00+02:00").build(),
            announcement("2137", "2026-09-10T09:51:00+02:00").build(),
        ];
        let journeys = build(&deps, &arrs, &Ticket::unrestricted());
        assert_eq!(
            journeys[0].arrival.advertised,
            time("2026-09-10T09:51:00+02:00")
        );
    }

    #[test]
    fn orders_by_when_the_train_is_expected_to_leave() {
        let deps = vec![
            announcement("A", "2026-09-10T09:10:00+02:00")
                .estimated("2026-09-10T09:40:00+02:00")
                .build(),
            announcement("B", "2026-09-10T09:20:00+02:00").build(),
        ];
        let arrs = vec![
            announcement("A", "2026-09-10T10:00:00+02:00").build(),
            announcement("B", "2026-09-10T10:05:00+02:00").build(),
        ];
        let journeys = build(&deps, &arrs, &Ticket::unrestricted());
        assert_eq!(
            journeys
                .iter()
                .map(|j| j.train.as_str())
                .collect::<Vec<_>>(),
            vec!["B", "A"]
        );
    }

    #[test]
    fn delay_is_measured_against_the_timetable() {
        let stop = Stop::from_announcement(
            &announcement("A", "2026-09-10T09:10:00+02:00")
                .estimated("2026-09-10T09:14:00+02:00")
                .build(),
        )
        .unwrap();
        assert_eq!(stop.delay_minutes(), 4);
        assert_eq!(stop.expected(), time("2026-09-10T09:14:00+02:00"));
    }

    fn journeys_for_selection() -> Vec<Journey> {
        let deps = vec![
            announcement("GONE", "2026-09-10T09:00:00+02:00")
                .product("Mälartåg")
                .actual("2026-09-10T09:01:00+02:00")
                .build(),
            announcement("FAST", "2026-09-10T09:20:00+02:00")
                .product("SJ Snabbtåg")
                .build(),
            announcement("CANC", "2026-09-10T09:25:00+02:00")
                .product("Mälartåg")
                .canceled()
                .build(),
            announcement("OK1", "2026-09-10T09:30:00+02:00")
                .product("Mälartåg")
                .build(),
            announcement("OK2", "2026-09-10T09:40:00+02:00")
                .product("SJ Regional")
                .build(),
        ];
        let arrs = vec![
            announcement("GONE", "2026-09-10T09:40:00+02:00").build(),
            announcement("FAST", "2026-09-10T09:58:00+02:00").build(),
            announcement("CANC", "2026-09-10T10:05:00+02:00").build(),
            announcement("OK1", "2026-09-10T10:09:00+02:00").build(),
            announcement("OK2", "2026-09-10T10:19:00+02:00").build(),
        ];
        build(&deps, &arrs, &movingo())
    }

    #[test]
    fn selection_keeps_only_boardable_trains() {
        let now = time("2026-09-10T09:10:00+02:00");
        let selection = select(journeys_for_selection(), now, 10, false);
        assert_eq!(
            selection
                .journeys
                .iter()
                .map(|j| j.train.as_str())
                .collect::<Vec<_>>(),
            vec!["OK1", "OK2"]
        );
        assert_eq!(
            selection.hidden,
            Hidden {
                departed: 1,
                canceled: 1,
                uncovered: 1
            }
        );
    }

    #[test]
    fn include_all_keeps_cancelled_and_uncovered_but_never_the_departed() {
        let now = time("2026-09-10T09:10:00+02:00");
        let selection = select(journeys_for_selection(), now, 10, true);
        assert_eq!(
            selection
                .journeys
                .iter()
                .map(|j| j.train.as_str())
                .collect::<Vec<_>>(),
            vec!["FAST", "CANC", "OK1", "OK2"]
        );
        assert_eq!(selection.hidden.departed, 1);
    }

    #[test]
    fn a_delayed_train_still_counts_as_catchable() {
        let deps = vec![
            announcement("LATE", "2026-09-10T09:05:00+02:00")
                .product("Mälartåg")
                .estimated("2026-09-10T09:18:00+02:00")
                .build(),
        ];
        let arrs = vec![announcement("LATE", "2026-09-10T09:44:00+02:00").build()];
        let journeys = build(&deps, &arrs, &movingo());
        let now = time("2026-09-10T09:10:00+02:00");
        let selection = select(journeys, now, 10, false);
        assert_eq!(selection.journeys.len(), 1);
        assert_eq!(selection.journeys[0].departure.delay_minutes(), 13);
    }

    #[test]
    fn selection_respects_the_limit() {
        let now = time("2026-09-10T09:10:00+02:00");
        let selection = select(journeys_for_selection(), now, 1, false);
        assert_eq!(selection.journeys.len(), 1);
        assert_eq!(selection.journeys[0].train, "OK1");
    }
}
