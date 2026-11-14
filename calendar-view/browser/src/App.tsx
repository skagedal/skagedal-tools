import { useCallback, useEffect, useMemo, useState } from "react";
import { CalEvent, Calendar, Overview, SyncStatus, askSync, fetchCalendars, fetchEvents, fetchVersion } from "./api";
import { addDays, iso, isoWeek, monday, parseDay, rangeTitle, sameDay, startOfDay, time } from "./dates";
import { Grid } from "./Grid";
import { Detail } from "./Detail";

type Mode = "week" | "day";

interface Place {
  day: Date;
  mode: Mode;
}

/** `#/2026-11-16/week`, so another page can link to a day. */
function readHash(): Place {
  const [, day, mode] = window.location.hash.split("/");
  return {
    day: (day && parseDay(day)) || startOfDay(new Date()),
    mode: mode === "day" ? "day" : "week",
  };
}

const HIDDEN_KEY = "calendar-view.toggled";

/** Keys toggled away from their configured default. */
function loadToggled(): Set<string> {
  try {
    return new Set(JSON.parse(localStorage.getItem(HIDDEN_KEY) ?? "[]"));
  } catch {
    return new Set();
  }
}

function saveToggled(toggled: Set<string>) {
  try {
    localStorage.setItem(HIDDEN_KEY, JSON.stringify([...toggled]));
  } catch {
    // Remembering is a convenience.
  }
}

const POLL_MS = 10_000;

function syncLabel(s: SyncStatus): string {
  if (s.running) return "Syncing…";
  if (!s.lastOk) return "Not synced yet";
  const at = new Date(s.lastOk);
  return sameDay(at, new Date()) ? `Synced ${time(at)}` : `Synced ${iso(at)} ${time(at)}`;
}

export function App() {
  const [place, setPlace] = useState<Place>(readHash);
  const [calendars, setCalendars] = useState<Calendar[] | null>(null);
  const [toggled, setToggled] = useState<Set<string>>(loadToggled);
  const [overview, setOverview] = useState<Overview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [version, setVersion] = useState<string | null>(null);
  const [sync, setSync] = useState<SyncStatus | null>(null);
  const [selected, setSelected] = useState<CalEvent | null>(null);

  useEffect(() => {
    fetchCalendars().then(setCalendars, (e) => setError(String(e.message ?? e)));
  }, []);

  useEffect(() => {
    const onHash = () => setPlace(readHash());
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);

  const go = useCallback((day: Date, mode: Mode) => {
    window.location.hash = `/${iso(day)}/${mode}`;
  }, []);

  // A week is always fetched, the day view showing one of its days, so
  // switching between them reads nothing new.
  const first = monday(place.day);
  const last = addDays(first, 6);
  const days = place.mode === "week" ? [...Array(7)].map((_, i) => addDays(first, i)) : [startOfDay(place.day)];

  const visible = useMemo(
    () => (calendars ?? []).map((c, i) => ({ c, i })).filter(({ c }) => c.shown !== toggled.has(c.key)),
    [calendars, toggled],
  );
  const which = visible.map(({ i }) => i).join(",");
  const firstIso = iso(first);

  useEffect(() => {
    if (!calendars) return;
    let live = true;
    const indices = which ? which.split(",").map(Number) : [];
    const lastIso = iso(addDays(parseDay(firstIso)!, 6));
    setLoading(true);
    fetchEvents(firstIso, lastIso, indices).then(
      (o) => {
        if (!live) return;
        setOverview(o);
        setError(null);
        setLoading(false);
        // Warm the server's cache for the weeks either side.
        for (const n of [-7, 7]) {
          const f = addDays(parseDay(firstIso)!, n);
          fetchEvents(iso(f), iso(addDays(f, 6)), indices).catch(() => {});
        }
      },
      (e) => {
        if (!live) return;
        setError(String(e.message ?? e));
        setLoading(false);
      },
    );
    return () => {
      live = false;
    };
  }, [calendars, which, firstIso, version]);

  const poll = useCallback(
    () =>
      fetchVersion().then(
        (v) => {
          setVersion(v.version);
          setSync(v.sync);
        },
        () => {},
      ),
    [],
  );

  useEffect(() => {
    poll();
    const id = window.setInterval(poll, POLL_MS);
    return () => window.clearInterval(id);
  }, [poll]);

  const syncNow = () => {
    setSync((s) => s && { ...s, running: true });
    askSync().then(() => window.setTimeout(poll, 1000), (e) => setError(String(e.message ?? e)));
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.metaKey || e.ctrlKey || e.altKey) return;
      const step = place.mode === "week" ? 7 : 1;
      if (e.key === "ArrowLeft" || e.key === "k") go(addDays(place.day, -step), place.mode);
      else if (e.key === "ArrowRight" || e.key === "j") go(addDays(place.day, step), place.mode);
      else if (e.key === "t") go(new Date(), place.mode);
      else if (e.key === "w") go(place.day, "week");
      else if (e.key === "d") go(place.day, "day");
      else if (e.key === "Escape") setSelected(null);
      else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [place, go]);

  const toggle = (key: string) => {
    const next = new Set(toggled);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    setToggled(next);
    saveToggled(next);
  };

  const byKey = useMemo(() => new Map((calendars ?? []).map((c) => [c.key, c])), [calendars]);
  const shownKeys = new Set(visible.map(({ c }) => c.key));
  const events = (overview?.events ?? []).filter((e) => shownKeys.has(e.calendar));
  const step = place.mode === "week" ? 7 : 1;
  const title = place.mode === "week" ? rangeTitle(first, last) : rangeTitle(days[0], days[0]);

  return (
    <div className="app">
      <aside className="sidebar">
        <h1>Calendars</h1>
        {!calendars && !error && <p className="muted">Loading…</p>}
        <ul className="calendars">
          {(calendars ?? []).map((c) => {
            const on = c.shown !== toggled.has(c.key);
            return (
              <li key={c.key}>
                <label className="check">
                  <input type="checkbox" checked={on} onChange={() => toggle(c.key)} style={{ accentColor: c.color }} />
                  <span className="swatch" style={{ background: c.color }} />
                  <span className="cal-label" title={`${c.account}: ${c.name}`}>{c.label}</span>
                </label>
              </li>
            );
          })}
        </ul>
        <p className="keys muted">← → move · t today · w week · d day</p>
      </aside>

      <main className="main">
        <header className="toolbar">
          <div className="nav">
            <button onClick={() => go(new Date(), place.mode)}>Today</button>
            <button aria-label="Previous" onClick={() => go(addDays(place.day, -step), place.mode)}>‹</button>
            <button aria-label="Next" onClick={() => go(addDays(place.day, step), place.mode)}>›</button>
          </div>
          <h2>
            {title} <span className="muted">· week {isoWeek(first)}</span>
          </h2>
          <span className={`spinner ${loading ? "on" : ""}`} aria-hidden={!loading}>Loading…</span>
          {sync?.configured && (
            <div className="sync">
              <span className="muted">{syncLabel(sync)}</span>
              <button onClick={syncNow} disabled={sync.running}>Sync</button>
            </div>
          )}
          <div className="modes" role="group" aria-label="View">
            <button className={place.mode === "day" ? "active" : ""} onClick={() => go(place.day, "day")}>Day</button>
            <button className={place.mode === "week" ? "active" : ""} onClick={() => go(place.day, "week")}>Week</button>
          </div>
        </header>

        {error && <div className="banner bad">{error}</div>}
        {sync?.lastError && <div className="banner bad">Sync failed: {sync.lastError}</div>}
        {overview?.failures.map((f) => (
          <div key={f.calendar} className="banner bad">
            Could not read {byKey.get(f.calendar)?.label ?? f.calendar}: {f.error}
          </div>
        ))}

        <Grid
          days={days}
          events={events}
          calendars={byKey}
          selected={selected}
          onSelect={setSelected}
          onDay={(d) => go(d, "day")}
        />
      </main>

      {selected && (
        <Detail event={selected} calendar={byKey.get(selected.calendar)} onClose={() => setSelected(null)} />
      )}
    </div>
  );
}
