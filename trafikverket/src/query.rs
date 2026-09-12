//! Building the XML request documents the Trafikverket API takes.
//!
//! The API is a JSON endpoint that consumes XML: every request is a
//! `<REQUEST>` document holding the authentication key and one `<QUERY>` per
//! object type, and the response comes back as JSON. Only the queries this
//! tool needs are built here.

/// Schema versions asked for. The API rejects a request that names a field
/// the given schema version doesn't have, so these are pinned rather than
/// left to the server's default.
const TRAIN_ANNOUNCEMENT_SCHEMA: &str = "1.9";
const TRAIN_STATION_SCHEMA: &str = "1.4";

/// `ActivityType` for a departure.
pub const DEPARTURE: &str = "Avgang";
/// `ActivityType` for an arrival.
pub const ARRIVAL: &str = "Ankomst";

/// A `TrainAnnouncement` query for one station, one activity type, and a
/// window expressed in minutes relative to now.
///
/// The window is left to the server through `$dateadd`, so a clock that
/// disagrees with Trafikverket's doesn't shift the results.
pub fn announcements(
    api_key: &str,
    location: &str,
    activity: &str,
    from_minutes: i64,
    to_minutes: i64,
) -> String {
    format!(
        "<REQUEST>\
           <LOGIN authenticationkey=\"{key}\"/>\
           <QUERY objecttype=\"TrainAnnouncement\" schemaversion=\"{schema}\" \
                  orderby=\"AdvertisedTimeAtLocation\">\
             <FILTER>\
               <AND>\
                 <EQ name=\"ActivityType\" value=\"{activity}\"/>\
                 <EQ name=\"LocationSignature\" value=\"{location}\"/>\
                 <EQ name=\"Advertised\" value=\"true\"/>\
                 <EQ name=\"Deleted\" value=\"false\"/>\
                 <GT name=\"AdvertisedTimeAtLocation\" value=\"{from}\"/>\
                 <LT name=\"AdvertisedTimeAtLocation\" value=\"{to}\"/>\
               </AND>\
             </FILTER>\
             <INCLUDE>AdvertisedTrainIdent</INCLUDE>\
             <INCLUDE>ScheduledDepartureDateTime</INCLUDE>\
             <INCLUDE>AdvertisedTimeAtLocation</INCLUDE>\
             <INCLUDE>EstimatedTimeAtLocation</INCLUDE>\
             <INCLUDE>TimeAtLocation</INCLUDE>\
             <INCLUDE>TrackAtLocation</INCLUDE>\
             <INCLUDE>Canceled</INCLUDE>\
             <INCLUDE>ToLocation</INCLUDE>\
             <INCLUDE>ProductInformation</INCLUDE>\
             <INCLUDE>Deviation</INCLUDE>\
             <INCLUDE>InformationOwner</INCLUDE>\
           </QUERY>\
         </REQUEST>",
        key = escape(api_key),
        schema = TRAIN_ANNOUNCEMENT_SCHEMA,
        activity = escape(activity),
        location = escape(location),
        from = date_add(from_minutes),
        to = date_add(to_minutes),
    )
}

/// A `TrainStation` query for every advertised station, used to turn
/// signatures into names and names into signatures.
pub fn stations(api_key: &str) -> String {
    format!(
        "<REQUEST>\
           <LOGIN authenticationkey=\"{key}\"/>\
           <QUERY objecttype=\"TrainStation\" schemaversion=\"{schema}\">\
             <FILTER>\
               <EQ name=\"Advertised\" value=\"true\"/>\
             </FILTER>\
             <INCLUDE>LocationSignature</INCLUDE>\
             <INCLUDE>AdvertisedLocationName</INCLUDE>\
           </QUERY>\
         </REQUEST>",
        key = escape(api_key),
        schema = TRAIN_STATION_SCHEMA,
    )
}

/// The default schema version for the object types this tool knows about.
/// Asking for the wrong one is how you get "Invalid query attribute", so a
/// type we have no default for has to be told explicitly.
pub fn default_schema(objecttype: &str) -> Option<&'static str> {
    match objecttype {
        "TrainAnnouncement" => Some(TRAIN_ANNOUNCEMENT_SCHEMA),
        "TrainStation" => Some(TRAIN_STATION_SCHEMA),
        _ => None,
    }
}

/// A query with no `INCLUDE` at all, so the API answers with every field the
/// object has. This is how you find out what a field is really called.
pub fn raw_object(
    api_key: &str,
    objecttype: &str,
    schema: &str,
    limit: Option<u32>,
    filter: Option<&str>,
) -> String {
    let limit = limit.map(|n| format!(" limit=\"{n}\"")).unwrap_or_default();
    let filter = filter
        .map(|f| format!("<FILTER>{f}</FILTER>"))
        .unwrap_or_default();
    wrap(
        api_key,
        &format!(
            "<QUERY objecttype=\"{}\" schemaversion=\"{}\"{limit}>{filter}</QUERY>",
            escape(objecttype),
            escape(schema),
        ),
    )
}

/// Put one or more `<QUERY>` elements into a `<REQUEST>` with the key. Any
/// `<REQUEST>` wrapper or `<LOGIN>` the caller already wrote is replaced, so
/// a document copied out of the API documentation works as it stands.
pub fn wrap(api_key: &str, document: &str) -> String {
    let mut body = document.trim();
    if let Some(rest) = body.strip_prefix("<REQUEST>") {
        body = rest
            .trim_end()
            .strip_suffix("</REQUEST>")
            .unwrap_or(rest)
            .trim();
    }
    let body = strip_login(body);
    format!(
        "<REQUEST><LOGIN authenticationkey=\"{}\"/>{}</REQUEST>",
        escape(api_key),
        body.trim()
    )
}

/// Remove a `<LOGIN .../>` element, whatever key it carries.
fn strip_login(document: &str) -> String {
    let Some(start) = document.find("<LOGIN") else {
        return document.to_string();
    };
    let Some(end) = document[start..].find('>') else {
        return document.to_string();
    };
    format!("{}{}", &document[..start], &document[start + end + 1..])
}

/// The API's `$dateadd` filter function, whose argument is a signed
/// `hh:mm:ss` offset from the server's current time.
fn date_add(minutes: i64) -> String {
    let sign = if minutes < 0 { "-" } else { "" };
    let total = minutes.unsigned_abs();
    format!("$dateadd({sign}{:02}:{:02}:00)", total / 60, total % 60)
}

/// Escape a value for use in XML text or an attribute value.
fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_add_formats_offsets() {
        assert_eq!(date_add(0), "$dateadd(00:00:00)");
        assert_eq!(date_add(30), "$dateadd(00:30:00)");
        assert_eq!(date_add(-30), "$dateadd(-00:30:00)");
        assert_eq!(date_add(185), "$dateadd(03:05:00)");
        assert_eq!(date_add(-1500), "$dateadd(-25:00:00)");
    }

    #[test]
    fn escapes_xml_metacharacters() {
        assert_eq!(escape("a&b<c>d\"e'f"), "a&amp;b&lt;c&gt;d&quot;e&apos;f");
        assert_eq!(escape("Uppsala C"), "Uppsala C");
    }

    #[test]
    fn announcement_query_carries_the_filter() {
        let xml = announcements("secret-key", "U", DEPARTURE, -30, 180);
        assert!(xml.contains("authenticationkey=\"secret-key\""));
        assert!(xml.contains("objecttype=\"TrainAnnouncement\""));
        assert!(xml.contains("<EQ name=\"ActivityType\" value=\"Avgang\"/>"));
        assert!(xml.contains("<EQ name=\"LocationSignature\" value=\"U\"/>"));
        assert!(
            xml.contains("<GT name=\"AdvertisedTimeAtLocation\" value=\"$dateadd(-00:30:00)\"/>")
        );
        assert!(
            xml.contains("<LT name=\"AdvertisedTimeAtLocation\" value=\"$dateadd(03:00:00)\"/>")
        );
        assert!(xml.contains("<INCLUDE>ProductInformation</INCLUDE>"));
        // Rows that are not for passengers, and ones withdrawn from the
        // timetable, are filtered out by the API rather than here.
        assert!(xml.contains("<EQ name=\"Advertised\" value=\"true\"/>"));
        assert!(xml.contains("<EQ name=\"Deleted\" value=\"false\"/>"));
    }

    #[test]
    fn station_query_asks_for_advertised_stations() {
        let xml = stations("k");
        assert!(xml.contains("objecttype=\"TrainStation\""));
        assert!(xml.contains("<EQ name=\"Advertised\" value=\"true\"/>"));
        assert!(xml.contains("<INCLUDE>AdvertisedLocationName</INCLUDE>"));
    }

    #[test]
    fn raw_object_asks_for_every_field() {
        let xml = raw_object("k", "TrainStation", "1.4", Some(1), None);
        assert!(
            xml.contains("<QUERY objecttype=\"TrainStation\" schemaversion=\"1.4\" limit=\"1\">")
        );
        assert!(!xml.contains("<INCLUDE>"));
        assert!(!xml.contains("<FILTER>"));
    }

    #[test]
    fn raw_object_takes_a_filter() {
        let xml = raw_object(
            "k",
            "TrainAnnouncement",
            "1.9",
            None,
            Some("<EQ name=\"LocationSignature\" value=\"U\"/>"),
        );
        assert!(xml.contains("<FILTER><EQ name=\"LocationSignature\" value=\"U\"/></FILTER>"));
        assert!(!xml.contains("limit="));
    }

    #[test]
    fn wrap_adds_the_login() {
        let xml = wrap("k", "<QUERY objecttype=\"TrainStation\"/>");
        assert_eq!(
            xml,
            "<REQUEST><LOGIN authenticationkey=\"k\"/><QUERY objecttype=\"TrainStation\"/></REQUEST>"
        );
    }

    #[test]
    fn wrap_replaces_a_request_and_login_already_written() {
        let xml = wrap(
            "k",
            "<REQUEST><LOGIN authenticationkey=\"theirs\"/><QUERY objecttype=\"X\"/></REQUEST>",
        );
        assert_eq!(
            xml,
            "<REQUEST><LOGIN authenticationkey=\"k\"/><QUERY objecttype=\"X\"/></REQUEST>"
        );
        assert!(!xml.contains("theirs"));
    }

    #[test]
    fn known_object_types_have_a_default_schema() {
        assert_eq!(default_schema("TrainAnnouncement"), Some("1.9"));
        assert_eq!(default_schema("TrainStation"), Some("1.4"));
        assert_eq!(default_schema("RoadCondition"), None);
    }

    #[test]
    fn api_key_is_escaped_into_the_document() {
        let xml = announcements("a\"b&c", "U", DEPARTURE, 0, 60);
        assert!(xml.contains("authenticationkey=\"a&quot;b&amp;c\""));
    }
}
