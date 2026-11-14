# calendar-view

Calendars, a week or a day at a time, in a window or a browser tab.
Everything it shows comes from [calendula](https://github.com/pimalaya/calendula)'s
`event list --json`, so the window, the CLI and anything else built on
calendula show the same occurrences, expanded by the same code. It only reads.

    calendar-view                     open the window
    calendar-view --serve             serve it, and print the URL, for a browser
    calendar-view events [--from D] [--to D] [--all]
                                      the events it would show, as JSON
    calendar-view calendars           the calendars, and which are shown at start

The page takes `#/<YYYY-MM-DD>/week` or `#/<YYYY-MM-DD>/day`, so another page
can link to a day. Arrow keys move, `t` goes to today, `w` and `d` switch view.

## Fast enough to click through

calendula's Microsoft Graph backend reads a calendar's whole history for any
range: about forty seconds for a week of a work calendar. So calendar-view
reads calendula accounts on the `pimdir` backend instead, a local store that
[neverest](https://github.com/pimalaya/neverest) keeps in sync with Google and
Graph. The same week then takes about a second. While the window is open it
runs `sync` (`neverest sync`) at start and every `sync-minutes`, and a Sync
button runs it at once. Events are kept in memory until one of the `watch`
files changes, which is how the page learns that a sync brought something new.

Without `sync` and `watch` it works against any calendula account, live ones
too, and keeps what it read for five minutes.

## Settings

`~/.config/skagedal-tools/calendar-view/settings.toml`, or the path in
`$CALENDAR_VIEW_CONFIG`. Without one, every calendar of every calendula
account is read and shown. See [settings.example.toml](settings.example.toml).

Cancelled occurrences are left out, and so are those where an address in
`me` declined. Graph's own "you declined" flag does not reach calendula's
iCalendar, so on Graph this works only where the attendee list says so.

## Development

    ./install calendar-view           builds the React app and embeds it
    cd browser && CALENDAR_VIEW_URL=http://127.0.0.1:<port>/ pnpm dev

The second proxies `/api` to a running `calendar-view --serve`.
