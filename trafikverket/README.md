# trafikverket

The next trains between two stations, with their live delays, from
[Trafikverket's open API](https://www.trafikverket.se/e-tjanster/trafikverkets-oppna-api-for-trafikinformation/).

Timetable apps answer a general question and leave you to do the filtering.
This one answers a narrow one: *which is the next train I can actually board,
and how late is it?*

```
$ trafikverket
Uppsala C → Stockholm C · Fri 11 Sep 06:35 · Mälartåg, SJ Regional

   in 4 min  06:40 → 07:17  Mälartåg 813    track 8a
  in 24 min  07:00 → 07:37  Mälartåg 10961  track 8a
  in 32 min  07:08 → 07:47  Mälartåg 915    track 4

4 departures hidden (4 not covered) — pass --all to see them.
```

## What it filters out

A departure your ticket does not cover is worse than no answer at all, so by
default the tool reports only trains it is sure about:

- **Products the ticket doesn't cover.** Validity on Swedish rail is decided
  by the train's product name, so a route in the configuration file carries
  the list of products its ticket covers. A train with no product information
  at all is treated as not boardable rather than assumed fine.
- **Cancelled departures.**
- **Trains going the other way, or not going all the way.** A departure counts
  only when the same train turns up among the arrivals at the destination.
  That is also where the arrival time comes from.
- **Trains that have already left** — including the ones whose timetabled
  time has passed. A train running 20 minutes late is still one you can catch,
  and it is reported with its forecast time.

`--all` shows the cancelled and uncovered ones, marked. Nothing brings back a
train that has gone.

## Getting a key

The data is CC0 but the endpoint needs a key, which is free. Register in
Trafikverket's data portal, [data.trafikverket.se](https://data.trafikverket.se),
and create a key under your account. Put it in the configuration file as
`api-key`, or in `$TRAFIKVERKET_API_KEY`, which wins.

`api.trafikinfo.trafikverket.se` is the API endpoint itself, not a sign-up
page — there is nothing to browse to there.

## Configuring a route

```console
$ trafikverket config edit
```

seeds `~/.config/skagedal-tools/trafikverket/config.toml` with a commented
template and opens it. A route is two station signatures and, optionally, the
products the ticket for that route covers:

```toml
api-key = "…"
default-route = "commute"

[route.commute]
from = "U"
to = "Cst"
products = ["Mälartåg", "SJ Regional"]
```

That example is a **Movingo** season ticket for the single route Uppsala to
Stockholm. It covers Mälartåg and SJ Regional. It does not cover SJ Snabbtåg
or the night trains, and SJ InterCity on the Linköping–Uppsala–Tierp line
needs the *Alla sträckor* ticket, so `"SJ InterCity"` belongs on the list only
if that is the ticket you hold. See
[Movingo's exceptions and limits](https://www.malardalstrafik.se/biljetter/movingo/undantag-och-begraensningar/).

A route with no `products` list reports every train.

Signatures are the API's own, not guesses:

```console
$ trafikverket stations uppsala
U  Uppsala C
```

The list is cached under `~/.cache/skagedal-tools/trafikverket/` and refreshed
monthly; `trafikverket stations --refresh` fetches it again. Anywhere a
station is asked for, a name works as well as a signature — an ambiguous one
is an error listing the candidates, never a guess.

## Usage

```
trafikverket [OPTIONS] [COMMAND]

  --route <NAME>       route from the configuration file
  --from <STATION>     origin, as a signature or a name
  --to <STATION>       destination, as a signature or a name
  -r, --reverse        travel the other way
  --product <NAME>     a product the ticket covers; repeat for several
  --any-product        report every train, whatever its product
  -n, --count <N>      how many departures to show (default 3)
  -w, --window <DUR>   how far ahead to look: 45m, 3h, 1h30m (default 3h)
  -a, --all            include cancelled and uncovered departures
  --json               print JSON instead of a table

  stations [QUERY]     list station signatures
  config [path|edit]   show or edit the configuration file
  raw                  post a query and print the reply
```

`--from`/`--to` name a route the configuration file says nothing about, so no
product list is carried over to it: a ticket for one route says nothing about
another. Give `--product` to filter such a route.

`--json` prints the same report as an object, so other tools can use it:

```console
$ trafikverket --json | jq -r '.journeys[0] | "\(.products[0]) \(.train) at \(.departure.estimated // .departure.advertised)"'
Mälartåg 2137 at 2026-09-10T07:19:00+02:00
```

## Asking the API directly

The API rejects a query that names a field the schema version does not have,
and the field names are not always the obvious ones — the timetable date is
`ScheduledDepartureDateTime`, and the time a train actually went is
`TimeAtLocation`, not `ActualTimeAtLocation`. `raw` is how you find that out:
it signs a query with your key, posts it, and prints the reply untouched.

```console
$ trafikverket raw --object TrainAnnouncement --limit 1 --pretty \
      --filter '<EQ name="LocationSignature" value="U"/>'
```

With no `INCLUDE` in the query, the API answers with every field the object
has, which is the list you want when something is called what you did not
expect. `--object` knows the schema version for the types this tool uses;
anything else needs `--schema`.

For full control, write the `<QUERY>` yourself and pass it in a file, or on
stdin with `-`. Any `<REQUEST>` wrapper or `<LOGIN>` element you include is
replaced with one carrying your configured key, so a document copied out of
the API documentation works as it stands:

```console
$ echo '<QUERY objecttype="TrainStation" schemaversion="1.4" limit="1"/>' |
      trafikverket raw --query -
```

## Notes

Queries go to `https://api.trafikinfo.trafikverket.se/v2/data.json` as XML
documents and come back as JSON, which is peculiar but documented. The tool
asks for `TrainAnnouncement` twice per run — departures at the origin,
arrivals at the destination — and lets the API's `$dateadd` decide the window,
so a clock that disagrees with Trafikverket's does not shift the results.
`$TRAFIKVERKET_API_ENDPOINT` points the client elsewhere, which is only useful
for testing against a stub.

A query the API rejects comes back as HTTP 400 carrying its own error object,
which names the field it objects to. That message is what the tool reports,
rather than the status line.
