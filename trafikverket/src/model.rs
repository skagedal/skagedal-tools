//! The slice of the Trafikverket response model this tool reads.

use chrono::{DateTime, FixedOffset};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ApiResponse {
    #[serde(rename = "RESPONSE")]
    pub response: ResponseBody,
}

#[derive(Debug, Deserialize)]
pub struct ResponseBody {
    #[serde(rename = "RESULT", default)]
    pub result: Vec<ResultItem>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ResultItem {
    #[serde(rename = "TrainAnnouncement", default)]
    pub train_announcements: Vec<TrainAnnouncement>,
    #[serde(rename = "TrainStation", default)]
    pub train_stations: Vec<TrainStation>,
    #[serde(rename = "ERROR")]
    pub error: Option<ApiError>,
}

#[derive(Debug, Deserialize)]
pub struct ApiError {
    #[serde(rename = "SOURCE")]
    pub source: Option<String>,
    #[serde(rename = "MESSAGE")]
    pub message: Option<String>,
}

/// One train at one station: either its departure or its arrival.
#[derive(Debug, Default, Deserialize)]
pub struct TrainAnnouncement {
    #[serde(rename = "AdvertisedTrainIdent")]
    pub train_ident: Option<String>,
    /// The date the train left its origin. Together with the train number
    /// this identifies a run, which matters around midnight when the same
    /// number is in the air on two dates at once.
    #[serde(rename = "ScheduledDepartureDateTime")]
    pub scheduled_departure_date: Option<String>,
    #[serde(rename = "AdvertisedTimeAtLocation")]
    pub advertised: Option<DateTime<FixedOffset>>,
    /// The current forecast. Absent when the train is running to plan.
    #[serde(rename = "EstimatedTimeAtLocation")]
    pub estimated: Option<DateTime<FixedOffset>>,
    /// When it actually happened. Set once the train has been and gone.
    #[serde(rename = "TimeAtLocation")]
    pub actual: Option<DateTime<FixedOffset>>,
    #[serde(rename = "TrackAtLocation")]
    pub track: Option<String>,
    #[serde(rename = "Canceled", default)]
    pub canceled: bool,
    #[serde(rename = "ToLocation", default)]
    pub to_location: Vec<LocationRef>,
    /// The product name, which is what decides whether a ticket covers the
    /// train: "Mälartåg", "SJ Regional", "SJ Snabbtåg" and so on.
    #[serde(rename = "ProductInformation", default)]
    pub product_information: Vec<CodedText>,
    #[serde(rename = "Deviation", default)]
    pub deviation: Vec<CodedText>,
    #[serde(rename = "InformationOwner")]
    pub information_owner: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LocationRef {
    #[serde(rename = "LocationName")]
    pub location_name: Option<String>,
    #[serde(rename = "Priority")]
    pub priority: Option<i64>,
}

/// A field the API has returned as a bare string in some schema versions and
/// as a `{Code, Description}` object in others.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum CodedText {
    Text(String),
    Coded {
        #[serde(rename = "Description")]
        description: Option<String>,
        #[serde(rename = "Code")]
        code: Option<String>,
    },
}

impl CodedText {
    /// The human-readable text, preferring the description over the code.
    pub fn text(&self) -> Option<&str> {
        match self {
            CodedText::Text(s) => Some(s.as_str()),
            CodedText::Coded { description, code } => description.as_deref().or(code.as_deref()),
        }
        .map(str::trim)
        .filter(|s| !s.is_empty())
    }
}

#[derive(Debug, Deserialize)]
pub struct TrainStation {
    #[serde(rename = "LocationSignature")]
    pub signature: Option<String>,
    #[serde(rename = "AdvertisedLocationName")]
    pub name: Option<String>,
}

impl TrainAnnouncement {
    /// Product names attached to this announcement, in the order given.
    pub fn products(&self) -> Vec<String> {
        self.product_information
            .iter()
            .filter_map(|p| p.text())
            .map(str::to_owned)
            .collect()
    }

    /// Deviation texts ("Inställd", "Kort tåg", …).
    pub fn deviations(&self) -> Vec<String> {
        self.deviation
            .iter()
            .filter_map(|d| d.text())
            .map(str::to_owned)
            .collect()
    }

    /// The advertised final destination: the highest-priority entry in
    /// `ToLocation`, which is the one shown on the departure board.
    pub fn destination(&self) -> Option<&str> {
        self.to_location
            .iter()
            .filter(|l| l.location_name.is_some())
            .min_by_key(|l| l.priority.unwrap_or(i64::MAX))
            .and_then(|l| l.location_name.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_coded_text_in_both_shapes() {
        let coded: Vec<CodedText> =
            serde_json::from_str(r#"[{"Code":"0","Description":"Mälartåg"},"SJ Regional"]"#)
                .unwrap();
        assert_eq!(coded[0].text(), Some("Mälartåg"));
        assert_eq!(coded[1].text(), Some("SJ Regional"));
    }

    #[test]
    fn coded_text_falls_back_to_code_and_drops_blanks() {
        let coded: Vec<CodedText> =
            serde_json::from_str(r#"[{"Code":"IC"},{"Description":"  "},""]"#).unwrap();
        assert_eq!(coded[0].text(), Some("IC"));
        assert_eq!(coded[1].text(), None);
        assert_eq!(coded[2].text(), None);
    }

    #[test]
    fn parses_an_announcement() {
        let json = r#"{
            "AdvertisedTrainIdent": "2137",
            "ScheduledDepartureDateTime": "2026-09-10T00:00:00.000+02:00",
            "AdvertisedTimeAtLocation": "2026-09-10T09:12:00.000+02:00",
            "EstimatedTimeAtLocation": "2026-09-10T09:16:00.000+02:00",
            "TrackAtLocation": "3",
            "ToLocation": [{"LocationName": "Nk", "Priority": 2}, {"LocationName": "Lp", "Priority": 1}],
            "ProductInformation": [{"Code": "1", "Description": "Mälartåg"}],
            "Deviation": [{"Code": "X", "Description": "Kort tåg"}]
        }"#;
        let a: TrainAnnouncement = serde_json::from_str(json).unwrap();
        assert_eq!(a.train_ident.as_deref(), Some("2137"));
        assert_eq!(a.products(), vec!["Mälartåg".to_string()]);
        assert_eq!(a.deviations(), vec!["Kort tåg".to_string()]);
        assert_eq!(a.destination(), Some("Lp"));
        assert!(!a.canceled);
        assert!(a.actual.is_none());
        assert_eq!(
            a.advertised.unwrap().to_rfc3339(),
            "2026-09-10T09:12:00+02:00"
        );
    }

    #[test]
    fn parses_an_error_response() {
        let json = r#"{"RESPONSE":{"RESULT":[{"ERROR":{"SOURCE":"filter","MESSAGE":"bad"}}]}}"#;
        let r: ApiResponse = serde_json::from_str(json).unwrap();
        let err = r.response.result[0].error.as_ref().unwrap();
        assert_eq!(err.message.as_deref(), Some("bad"));
    }
}
