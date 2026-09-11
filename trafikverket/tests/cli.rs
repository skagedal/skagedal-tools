//! End-to-end tests: the real binary, talking to a stub of the API.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Output};

use chrono::{DateTime, Duration, Local};

/// Start a stub of the Trafikverket endpoint and return its base URL. The
/// thread serving it lives as long as the test process.
fn start_stub() -> String {
    let listener = TcpListener::bind::<SocketAddr>("127.0.0.1:0".parse().unwrap()).unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            std::thread::spawn(move || serve(stream));
        }
    });
    format!("http://{address}/v2/data.json")
}

fn serve(mut stream: TcpStream) {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    let body = loop {
        let read = match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        buffer.extend_from_slice(&chunk[..read]);
        let text = String::from_utf8_lossy(&buffer).to_string();
        let Some(header_end) = text.find("\r\n\r\n") else {
            continue;
        };
        let length: usize = text[..header_end]
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse().ok())?
            })
            .unwrap_or(0);
        let body = &text[header_end + 4..];
        if body.len() >= length {
            break body.to_string();
        }
    };

    let payload = respond_to(&body);
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn respond_to(request: &str) -> String {
    assert!(
        request.contains("authenticationkey=\"test-key\""),
        "the binary should send the configured key: {request}"
    );
    if request.contains("objecttype=\"TrainStation\"") {
        return stations_response();
    }
    assert!(
        request.contains("objecttype=\"TrainAnnouncement\""),
        "unexpected query: {request}"
    );
    // The stub knows one direction only: departures from Uppsala and
    // arrivals at Stockholm. Anything else is an empty timetable.
    match (
        request.contains("value=\"Avgang\""),
        request.contains("<EQ name=\"LocationSignature\" value=\"U\"/>"),
    ) {
        (true, true) => departures_response(),
        (false, false) => arrivals_response(),
        _ => empty_response(),
    }
}

fn empty_response() -> String {
    r#"{"RESPONSE":{"RESULT":[{"TrainAnnouncement":[]}]}}"#.to_string()
}

fn stations_response() -> String {
    r#"{"RESPONSE":{"RESULT":[{"TrainStation":[
        {"LocationSignature":"U","AdvertisedLocationName":"Uppsala C"},
        {"LocationSignature":"Cst","AdvertisedLocationName":"Stockholm C"},
        {"LocationSignature":"Gä","AdvertisedLocationName":"Gävle C"}
    ]}]}}"#
        .to_string()
}

/// A time a given number of minutes from now, in the format the API uses.
fn at(minutes: i64) -> String {
    let time: DateTime<Local> = Local::now() + Duration::minutes(minutes);
    time.format("%Y-%m-%dT%H:%M:%S%.3f%:z").to_string()
}

fn announcement(fields: &[(&str, String)]) -> String {
    let body: Vec<String> = fields
        .iter()
        .map(|(name, value)| format!("\"{name}\":{value}"))
        .collect();
    format!("{{{}}}", body.join(","))
}

fn quoted(value: &str) -> String {
    format!("\"{value}\"")
}

fn product(name: &str) -> String {
    format!("[{{\"Code\":\"0\",\"Description\":\"{name}\"}}]")
}

fn departures_response() -> String {
    let rows = [
        // Covered, on time.
        announcement(&[
            ("AdvertisedTrainIdent", quoted("2137")),
            ("ScheduledDepartureDate", quoted("2026-09-10")),
            ("AdvertisedTimeAtLocation", quoted(&at(12))),
            ("TrackAtLocation", quoted("3")),
            ("ProductInformation", product("Mälartåg")),
        ]),
        // Not covered by a Movingo single-route ticket.
        announcement(&[
            ("AdvertisedTrainIdent", quoted("424")),
            ("ScheduledDepartureDate", quoted("2026-09-10")),
            ("AdvertisedTimeAtLocation", quoted(&at(20))),
            ("TrackAtLocation", quoted("6")),
            ("ProductInformation", product("SJ Snabbtåg")),
        ]),
        // Covered, but heading north: it never arrives at Stockholm C.
        announcement(&[
            ("AdvertisedTrainIdent", quoted("8801")),
            ("ScheduledDepartureDate", quoted("2026-09-10")),
            ("AdvertisedTimeAtLocation", quoted(&at(25))),
            ("ProductInformation", product("Mälartåg")),
        ]),
        // Covered, cancelled.
        announcement(&[
            ("AdvertisedTrainIdent", quoted("2139")),
            ("ScheduledDepartureDate", quoted("2026-09-10")),
            ("AdvertisedTimeAtLocation", quoted(&at(40))),
            ("Canceled", "true".to_string()),
            ("ProductInformation", product("Mälartåg")),
        ]),
        // Covered, running late.
        announcement(&[
            ("AdvertisedTrainIdent", quoted("634")),
            ("ScheduledDepartureDate", quoted("2026-09-10")),
            ("AdvertisedTimeAtLocation", quoted(&at(50))),
            ("EstimatedTimeAtLocation", quoted(&at(54))),
            ("TrackAtLocation", quoted("9")),
            ("ProductInformation", product("SJ Regional")),
        ]),
        // Covered, but it left before we asked.
        announcement(&[
            ("AdvertisedTrainIdent", quoted("2135")),
            ("ScheduledDepartureDate", quoted("2026-09-10")),
            ("AdvertisedTimeAtLocation", quoted(&at(-14))),
            ("ActualTimeAtLocation", quoted(&at(-14))),
            ("ProductInformation", product("Mälartåg")),
        ]),
    ];
    format!(
        "{{\"RESPONSE\":{{\"RESULT\":[{{\"TrainAnnouncement\":[{}]}}]}}}}",
        rows.join(",")
    )
}

fn arrivals_response() -> String {
    let rows = [
        ("2137", 51, None),
        ("424", 38, None),
        ("2139", 79, None),
        ("634", 89, Some(93)),
        ("2135", 25, None),
    ];
    let rows: Vec<String> = rows
        .iter()
        .map(|(ident, advertised, estimated)| {
            let mut fields = vec![
                ("AdvertisedTrainIdent", quoted(ident)),
                ("ScheduledDepartureDate", quoted("2026-09-10")),
                ("AdvertisedTimeAtLocation", quoted(&at(*advertised))),
            ];
            if let Some(estimated) = estimated {
                fields.push(("EstimatedTimeAtLocation", quoted(&at(*estimated))));
            }
            announcement(&fields)
        })
        .collect();
    format!(
        "{{\"RESPONSE\":{{\"RESULT\":[{{\"TrainAnnouncement\":[{}]}}]}}}}",
        rows.join(",")
    )
}

const CONFIG: &str = r#"
default-route = "commute"

[route.commute]
from = "U"
to = "Cst"
products = ["Mälartåg", "SJ Regional"]
"#;

struct Fixture {
    home: tempfile::TempDir,
    endpoint: String,
}

impl Fixture {
    fn new() -> Fixture {
        let home = tempfile::tempdir().unwrap();
        let config = home
            .path()
            .join("config")
            .join("skagedal-tools")
            .join("trafikverket");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(config.join("config.toml"), CONFIG).unwrap();
        Fixture {
            home,
            endpoint: start_stub(),
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        let root: &Path = self.home.path();
        Command::new(env!("CARGO_BIN_EXE_trafikverket"))
            .args(args)
            .env("XDG_CONFIG_HOME", root.join("config"))
            .env("XDG_CACHE_HOME", root.join("cache"))
            .env("TRAFIKVERKET_API_KEY", "test-key")
            .env("TRAFIKVERKET_API_ENDPOINT", &self.endpoint)
            .env("NO_COLOR", "1")
            // The stub is on loopback; a proxy in the environment must not
            // get in the way.
            .env("NO_PROXY", "127.0.0.1,localhost")
            .env_remove("HTTP_PROXY")
            .env_remove("http_proxy")
            .env_remove("ALL_PROXY")
            .env_remove("all_proxy")
            .output()
            .unwrap()
    }

    fn stdout(&self, args: &[&str]) -> String {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "`{}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }
}

#[test]
fn reports_only_the_trains_the_ticket_covers() {
    let fixture = Fixture::new();
    let out = fixture.stdout(&[]);

    assert!(out.contains("Uppsala C → Stockholm C"), "{out}");
    assert!(out.contains("Mälartåg 2137"), "{out}");
    assert!(out.contains("track 3"), "{out}");
    assert!(out.contains("SJ Regional 634"), "{out}");
    assert!(out.contains("4 min late"), "{out}");

    // Not covered, cancelled, wrong direction, already gone.
    assert!(!out.contains("424"), "{out}");
    assert!(!out.contains("2139"), "{out}");
    assert!(!out.contains("8801"), "{out}");
    assert!(!out.contains("2135"), "{out}");

    assert!(
        out.contains("2 departures hidden (1 not covered, 1 cancelled)"),
        "{out}"
    );
}

#[test]
fn all_shows_what_was_filtered_out_but_not_what_has_left() {
    let fixture = Fixture::new();
    let out = fixture.stdout(&["--all"]);
    assert!(out.contains("SJ Snabbtåg 424"), "{out}");
    assert!(out.contains("not covered by this ticket"), "{out}");
    assert!(out.contains("cancelled"), "{out}");
    assert!(!out.contains("8801"), "{out}");
    assert!(!out.contains("2135"), "{out}");
}

#[test]
fn count_limits_the_answer_and_says_so() {
    let fixture = Fixture::new();
    let out = fixture.stdout(&["-n", "1"]);
    assert!(out.contains("Mälartåg 2137"), "{out}");
    assert!(!out.contains("SJ Regional 634"), "{out}");
    assert!(
        out.contains("1 more within 3 h — raise -n to see it."),
        "{out}"
    );
}

#[test]
fn a_count_that_fits_says_nothing_about_more() {
    let fixture = Fixture::new();
    let out = fixture.stdout(&[]);
    assert!(!out.contains("raise -n"), "{out}");
}

#[test]
fn any_product_drops_the_ticket_filter() {
    let fixture = Fixture::new();
    let out = fixture.stdout(&["--any-product"]);
    assert!(out.contains("SJ Snabbtåg 424"), "{out}");
    assert!(!out.contains("not covered"), "{out}");
}

#[test]
fn json_output_is_machine_readable() {
    let fixture = Fixture::new();
    let out = fixture.stdout(&["--json"]);
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(value["from"]["signature"], "U");
    assert_eq!(value["to"]["name"], "Stockholm C");
    assert_eq!(value["journeys"][0]["train"], "2137");
    assert_eq!(value["journeys"][0]["coverage"], "covered");
    assert_eq!(value["journeys"][1]["train"], "634");
    assert_eq!(value["hidden"]["uncovered"], 1);
    assert_eq!(value["hidden"]["canceled"], 1);
    assert_eq!(value["hidden"]["departed"], 1);
}

#[test]
fn reverse_swaps_the_ends() {
    let fixture = Fixture::new();
    // The stub only answers for U departures and Cst arrivals, so the
    // reversed route finds nothing — which is itself the check that the
    // signatures were swapped.
    let out = fixture.stdout(&["--reverse"]);
    assert!(out.contains("Stockholm C → Uppsala C"), "{out}");
    assert!(out.contains("No departures to Uppsala C"), "{out}");
}

#[test]
fn stations_are_looked_up_by_name() {
    let fixture = Fixture::new();
    let out = fixture.stdout(&["stations", "uppsala"]);
    assert_eq!(out, "U  Uppsala C\n");
}

#[test]
fn the_station_list_is_cached_between_runs() {
    let fixture = Fixture::new();
    fixture.stdout(&["stations", "uppsala"]);
    let cache = fixture
        .home
        .path()
        .join("cache")
        .join("skagedal-tools")
        .join("trafikverket")
        .join("stations.json");
    assert!(cache.exists(), "expected a cache at {}", cache.display());
    assert!(
        std::fs::read_to_string(&cache)
            .unwrap()
            .contains("Uppsala C")
    );
}

#[test]
fn an_ad_hoc_route_takes_station_names() {
    let fixture = Fixture::new();
    let out = fixture.stdout(&["--from", "Uppsala C", "--to", "Stockholm C"]);
    assert!(out.contains("Uppsala C → Stockholm C"), "{out}");
    // No route from the file, so no product filter: the fast train shows up.
    assert!(out.contains("SJ Snabbtåg 424"), "{out}");
}

#[test]
fn an_unknown_station_is_an_error_that_suggests_a_lookup() {
    let fixture = Fixture::new();
    let output = fixture.run(&["--from", "Kabul", "--to", "Cst"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no station matches"), "{stderr}");
}

#[test]
fn a_missing_api_key_says_where_to_get_one() {
    let home = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_trafikverket"))
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_CACHE_HOME", home.path().join("cache"))
        .env_remove("TRAFIKVERKET_API_KEY")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("https://data.trafikverket.se"), "{stderr}");
}

#[test]
fn config_path_points_into_the_xdg_config_directory() {
    let fixture = Fixture::new();
    let out = fixture.stdout(&["config", "path"]);
    assert!(
        out.trim()
            .ends_with("skagedal-tools/trafikverket/config.toml"),
        "{out}"
    );
}
