import { useMemo, useState } from "react";
import { useHotkeys } from "react-hotkeys-hook";
import { useLocalStorage } from "usehooks-ts";
import { CalEvent, SyncStatus } from "./api";
import { Detail } from "./Detail";
import { Grid } from "./Grid";
import { Mode, useCalendars, useHashPlace, useSync, useVersion, useWeek } from "./hooks";
import { isoWeek, monday, rangeTitle, time, today, zoned } from "./time";

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

function syncLabel(s: SyncStatus): string {
  if (s.running) return "Syncing…";
  if (!s.lastOk) return "Not synced yet";
  const at = zoned(s.lastOk);
  return at.toPlainDate().equals(today()) ? `Synced ${time(at)}` : `Synced ${at.toPlainDate()} ${time(at)}`;
}

export function App() {
  const [place, go] = useHashPlace();
  // Keys toggled away from their configured default.
  const [toggled, setToggled] = useLocalStorage<string[]>("calendar-view.toggled", []);
  const [selected, setSelected] = useState<CalEvent | null>(null);

  const calendars = useCalendars();
  const sync = useVersion().data?.sync;
  const syncNow = useSync();

  const first = monday(place.day);
  const last = first.add({ days: 6 });
  // A week is always read, the day view showing one of its days, so
  // switching between them reads nothing new.
  const days =
    place.mode === "week" ? Array.from({ length: 7 }, (_, i) => first.add({ days: i })) : [place.day];

  const visible = useMemo(
    () => (calendars.data ?? []).map((c, i) => ({ c, i })).filter(({ c }) => c.shown !== toggled.includes(c.key)),
    [calendars.data, toggled],
  );
  const week = useWeek(first, visible.map(({ i }) => i), calendars.isSuccess);

  const step = { days: place.mode === "week" ? 7 : 1 };
  const move = (to: Temporal.PlainDate, mode: Mode = place.mode) => go({ day: to, mode });
  useHotkeys("left, k", () => move(place.day.subtract(step)), { preventDefault: true });
  useHotkeys("right, j", () => move(place.day.add(step)), { preventDefault: true });
  useHotkeys("t", () => move(today()));
  useHotkeys("w", () => move(place.day, "week"));
  useHotkeys("d", () => move(place.day, "day"));
  useHotkeys("escape", () => setSelected(null));

  const toggle = (key: string) =>
    setToggled((keys) => (keys.includes(key) ? keys.filter((k) => k !== key) : [...keys, key]));

  const byKey = useMemo(() => new Map((calendars.data ?? []).map((c) => [c.key, c])), [calendars.data]);
  const shownKeys = new Set(visible.map(({ c }) => c.key));
  const events = (week.data?.events ?? []).filter((e) => shownKeys.has(e.calendar));
  const title = place.mode === "week" ? rangeTitle(first, last) : rangeTitle(place.day, place.day);
  const error = calendars.error ?? week.error ?? syncNow.error;

  return (
    <div className="app">
      <aside className="sidebar">
        <h1>Calendars</h1>
        {calendars.isPending && <p className="muted">Loading…</p>}
        <ul className="calendars">
          {(calendars.data ?? []).map((c) => (
            <li key={c.key}>
              <label className="check">
                <input
                  type="checkbox"
                  checked={c.shown !== toggled.includes(c.key)}
                  onChange={() => toggle(c.key)}
                  style={{ accentColor: c.color }}
                />
                <span className="swatch" style={{ background: c.color }} />
                <span className="cal-label" title={`${c.account}: ${c.name}`}>{c.label}</span>
              </label>
            </li>
          ))}
        </ul>
        <p className="keys muted">← → move · t today · w week · d day</p>
      </aside>

      <main className="main">
        <header className="toolbar">
          <div className="nav">
            <button onClick={() => move(today())}>Today</button>
            <button aria-label="Previous" onClick={() => move(place.day.subtract(step))}>‹</button>
            <button aria-label="Next" onClick={() => move(place.day.add(step))}>›</button>
          </div>
          <h2>
            {title} <span className="muted">· week {isoWeek(first)}</span>
          </h2>
          <span className={`spinner ${week.isFetching ? "on" : ""}`} aria-hidden={!week.isFetching}>Loading…</span>
          {sync?.configured && (
            <div className="sync">
              <span className="muted">{syncLabel(sync)}</span>
              <button onClick={() => syncNow.mutate()} disabled={sync.running || syncNow.isPending}>Sync</button>
            </div>
          )}
          <div className="modes" role="group" aria-label="View">
            <button className={place.mode === "day" ? "active" : ""} onClick={() => move(place.day, "day")}>Day</button>
            <button className={place.mode === "week" ? "active" : ""} onClick={() => move(place.day, "week")}>Week</button>
          </div>
        </header>

        {error && <div className="banner bad">{message(error)}</div>}
        {sync?.lastError && <div className="banner bad">Sync failed: {sync.lastError}</div>}
        {week.data?.failures.map((f) => (
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
          onDay={(d) => move(d, "day")}
        />
      </main>

      {selected && (
        <Detail event={selected} calendar={byKey.get(selected.calendar)} onClose={() => setSelected(null)} />
      )}
    </div>
  );
}
