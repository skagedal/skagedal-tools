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
                 <GT name=\"AdvertisedTimeAtLocation\" value=\"{from}\"/>\
                 <LT name=\"AdvertisedTimeAtLocation\" value=\"{to}\"/>\
               </AND>\
             </FILTER>\
             <INCLUDE>AdvertisedTrainIdent</INCLUDE>\
             <INCLUDE>ScheduledDepartureDate</INCLUDE>\
             <INCLUDE>AdvertisedTimeAtLocation</INCLUDE>\
             <INCLUDE>EstimatedTimeAtLocation</INCLUDE>\
             <INCLUDE>ActualTimeAtLocation</INCLUDE>\
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
    }

    #[test]
    fn station_query_asks_for_advertised_stations() {
        let xml = stations("k");
        assert!(xml.contains("objecttype=\"TrainStation\""));
        assert!(xml.contains("<EQ name=\"Advertised\" value=\"true\"/>"));
        assert!(xml.contains("<INCLUDE>AdvertisedLocationName</INCLUDE>"));
    }

    #[test]
    fn api_key_is_escaped_into_the_document() {
        let xml = announcements("a\"b&c", "U", DEPARTURE, 0, 60);
        assert!(xml.contains("authenticationkey=\"a&quot;b&amp;c\""));
    }
}
