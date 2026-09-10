//! The HTTP client for Trafikverket's open API.

use anyhow::{Context, Result, anyhow, bail};

use crate::model::{ApiResponse, ResultItem, TrainAnnouncement, TrainStation};
use crate::query;

pub const ENDPOINT: &str = "https://api.trafikinfo.trafikverket.se/v2/data.json";

/// Points the client somewhere else. Only useful for testing against a stub
/// of the API.
pub const ENDPOINT_ENV: &str = "TRAFIKVERKET_API_ENDPOINT";

const USER_AGENT: &str = concat!("skagedal-tools trafikverket/", env!("CARGO_PKG_VERSION"));

pub struct Client {
    http: reqwest::Client,
    api_key: String,
    endpoint: String,
}

impl Client {
    pub fn new(api_key: String) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .context("could not build an HTTP client")?;
        let endpoint = std::env::var(ENDPOINT_ENV)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| ENDPOINT.to_string());
        Ok(Client {
            http,
            api_key,
            endpoint,
        })
    }

    /// Departures or arrivals at one station, within a window given in
    /// minutes relative to now.
    pub async fn announcements(
        &self,
        location: &str,
        activity: &str,
        from_minutes: i64,
        to_minutes: i64,
    ) -> Result<Vec<TrainAnnouncement>> {
        let body =
            query::announcements(&self.api_key, location, activity, from_minutes, to_minutes);
        let results = self.post(body).await?;
        Ok(results
            .into_iter()
            .flat_map(|r| r.train_announcements)
            .collect())
    }

    /// Every advertised train station.
    pub async fn stations(&self) -> Result<Vec<TrainStation>> {
        let results = self.post(query::stations(&self.api_key)).await?;
        Ok(results.into_iter().flat_map(|r| r.train_stations).collect())
    }

    async fn post(&self, body: String) -> Result<Vec<ResultItem>> {
        let response = self
            .http
            .post(&self.endpoint)
            .header(reqwest::header::CONTENT_TYPE, "text/xml")
            .body(body)
            .send()
            .await
            .with_context(|| format!("could not reach {}", self.endpoint))?;

        let status = response.status();
        let text = response
            .text()
            .await
            .context("could not read the API response")?;

        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            bail!(
                "Trafikverket rejected the API key ({status}). \
                 Get one at https://api.trafikinfo.trafikverket.se and set it \
                 in the config file or in ${}",
                crate::config::API_KEY_ENV
            );
        }
        if !status.is_success() {
            bail!("{} returned {status}: {}", self.endpoint, excerpt(&text));
        }
        parse_response(&text)
    }
}

/// Parse a response body, turning the API's own error object into an error.
pub fn parse_response(text: &str) -> Result<Vec<ResultItem>> {
    let parsed: ApiResponse = serde_json::from_str(text)
        .with_context(|| format!("could not parse the API response: {}", excerpt(text)))?;
    for item in &parsed.response.result {
        if let Some(error) = &item.error {
            let message = error.message.as_deref().unwrap_or("no message given");
            return match error.source.as_deref() {
                Some(source) => Err(anyhow!(
                    "Trafikverket rejected the query ({source}): {message}"
                )),
                None => Err(anyhow!("Trafikverket rejected the query: {message}")),
            };
        }
    }
    Ok(parsed.response.result)
}

/// A short, single-line sample of a response body, for error messages.
fn excerpt(text: &str) -> String {
    let line = text.trim().lines().next().unwrap_or("").trim();
    if line.chars().count() <= 200 {
        return line.to_string();
    }
    let head: String = line.chars().take(200).collect();
    format!("{head}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_announcements_out_of_a_response() {
        let body = r#"{"RESPONSE":{"RESULT":[{"TrainAnnouncement":[
            {"AdvertisedTrainIdent":"2137","AdvertisedTimeAtLocation":"2026-09-10T09:12:00.000+02:00"}
        ]}]}}"#;
        let results = parse_response(body).unwrap();
        let announcements: Vec<_> = results
            .into_iter()
            .flat_map(|r| r.train_announcements)
            .collect();
        assert_eq!(announcements.len(), 1);
        assert_eq!(announcements[0].train_ident.as_deref(), Some("2137"));
    }

    #[test]
    fn an_empty_result_set_is_not_an_error() {
        let results =
            parse_response(r#"{"RESPONSE":{"RESULT":[{"TrainAnnouncement":[]}]}}"#).unwrap();
        assert!(results[0].train_announcements.is_empty());
    }

    #[test]
    fn surfaces_the_api_error_object() {
        let body = r#"{"RESPONSE":{"RESULT":[{"ERROR":{"SOURCE":"filter",
            "MESSAGE":"Unknown field TrackAtLoc"}}]}}"#;
        let err = parse_response(body).unwrap_err();
        let text = format!("{err:#}");
        assert!(text.contains("filter"), "{text}");
        assert!(text.contains("Unknown field TrackAtLoc"), "{text}");
    }

    #[test]
    fn a_non_json_body_is_reported_with_an_excerpt() {
        let err = parse_response("<html>gateway timeout</html>").unwrap_err();
        assert!(format!("{err:#}").contains("gateway timeout"));
    }

    #[test]
    fn excerpt_is_one_short_line() {
        assert_eq!(excerpt("  hello\nworld  "), "hello");
        let long = "x".repeat(500);
        let short = excerpt(&long);
        assert_eq!(short.chars().count(), 201);
        assert!(short.ends_with('…'));
    }
}
