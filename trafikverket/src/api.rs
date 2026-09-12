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

    /// Post a `<QUERY>` (or a whole `<REQUEST>`) and hand back the response
    /// body as it came, without interpreting it. For working out what the API
    /// really calls things.
    pub async fn raw(&self, document: &str) -> Result<String> {
        self.send(query::wrap(&self.api_key, document)).await
    }

    /// Every field of an object type, which is the question `raw` exists for.
    pub async fn raw_object(
        &self,
        objecttype: &str,
        schema: &str,
        limit: Option<u32>,
        filter: Option<&str>,
    ) -> Result<String> {
        self.send(query::raw_object(
            &self.api_key,
            objecttype,
            schema,
            limit,
            filter,
        ))
        .await
    }

    /// Ask for the smallest answer the API will give, to find out whether
    /// the key is one it accepts. A key it rejects comes back as an error
    /// from `send`, so a successful reply is the whole result.
    pub async fn check_key(&self) -> Result<()> {
        let schema = query::default_schema("TrainStation").expect("TrainStation has a schema");
        self.raw_object("TrainStation", schema, Some(1), None)
            .await?;
        Ok(())
    }

    async fn post(&self, body: String) -> Result<Vec<ResultItem>> {
        let text = self.send(body).await?;
        parse_response(&text)
    }

    /// Post a document and return the response body. An API error in the body
    /// is raised here; anything else is left for the caller to make sense of.
    async fn send(&self, body: String) -> Result<String> {
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
                 Get one from Trafikverket's data portal at \
                 https://data.trafikverket.se, then run `trafikverket auth` \
                 (or set ${})",
                crate::config::API_KEY_ENV
            );
        }
        // A rejected query comes back as 400 carrying the API's own ERROR
        // object, which names the field it objects to. That says far more than
        // the status line, so a JSON body is read first either way.
        if text.trim_start().starts_with('{') {
            parse_response(&text)?;
            if status.is_success() {
                return Ok(text);
            }
        }
        bail!("{} returned {status}: {}", self.endpoint, excerpt(&text))
    }
}

/// Parse a response body, turning the API's own error object into an error.
pub fn parse_response(text: &str) -> Result<Vec<ResultItem>> {
    let parsed: ApiResponse = serde_json::from_str(text)
        .with_context(|| format!("could not parse the API response: {}", excerpt(text)))?;
    match first_error(&parsed) {
        Some(error) => Err(error),
        None => Ok(parsed.response.result),
    }
}

/// The first error the API reported, if it reported one.
fn first_error(parsed: &ApiResponse) -> Option<anyhow::Error> {
    let error = parsed
        .response
        .result
        .iter()
        .find_map(|r| r.error.as_ref())?;
    let message = error.message.as_deref().unwrap_or("no message given");
    Some(match error.source.as_deref() {
        Some(source) => anyhow!("Trafikverket rejected the query ({source}): {message}"),
        None => anyhow!("Trafikverket rejected the query: {message}"),
    })
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
