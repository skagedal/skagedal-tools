import { useCallback } from "react";
import { CalEvent, Calendar } from "./api";
import { useNow } from "./hooks";
import { Timed, allDayOn, minutesInto, timedOn } from "./layout";
import { time, weekday } from "./time";

const HOUR_PX = 48;
const FIRST_HOUR_IN_VIEW = 7;

interface Props {
  days: Temporal.PlainDate[];
  events: CalEvent[];
  calendars: Map<string, Calendar>;
  selected: CalEvent | null;
  onSelect: (e: CalEvent) => void;
  onDay: (d: Temporal.PlainDate) => void;
}

const same = (a: CalEvent | null, b: CalEvent) =>
  !!a && a.calendar === b.calendar && a.id === b.id && a.recurrenceId === b.recurrenceId && a.start === b.start;

const colorOf = (calendars: Map<string, Calendar>, e: CalEvent) => calendars.get(e.calendar)?.color ?? "var(--muted)";

/** Free or tentative time is drawn lighter than busy time. */
function tone(e: CalEvent): string {
  if (e.transparency === "TRANSPARENT" || e.busyStatus === "FREE") return "free";
  if (e.busyStatus === "TENTATIVE") return "tentative";
  return "";
}

export function Grid({ days, events, calendars, selected, onSelect, onDay }: Props) {
  const now = useNow();
  const today = now.toPlainDate();
  // Scrolled to the morning once, when the grid first appears.
  const scroller = useCallback((el: HTMLDivElement | null) => {
    if (el) el.scrollTop = FIRST_HOUR_IN_VIEW * HOUR_PX;
  }, []);

  const columns = { gridTemplateColumns: `56px repeat(${days.length}, minmax(0, 1fr))` };
  const allDay = days.map((d) => allDayOn(events, d));
  const timed = days.map((d) => timedOn(events, d));

  return (
    <div className="grid">
      <div className="grid-head" style={columns}>
        <div />
        {days.map((d) => (
          <button key={d.toString()} className={`day-head ${d.equals(today) ? "today" : ""}`} onClick={() => onDay(d)}>
            <span className="dow">{weekday(d)}</span>
            <span className="dom">{d.day}</span>
          </button>
        ))}
      </div>

      <div className="all-day" style={columns}>
        <div className="gutter-label">all day</div>
        {allDay.map((list, i) => (
          <div key={i} className="all-day-cell">
            {list.map((e) => (
              <button
                key={`${e.calendar}/${e.id}/${e.start}`}
                className={`chip ${tone(e)} ${same(selected, e) ? "selected" : ""}`}
                style={{ "--c": colorOf(calendars, e) } as React.CSSProperties}
                onClick={() => onSelect(e)}
                title={e.summary ?? ""}
              >
                {e.summary || "(no title)"}
              </button>
            ))}
          </div>
        ))}
      </div>

      <div className="scroller" ref={scroller}>
        <div className="body" style={{ ...columns, height: 24 * HOUR_PX }}>
          <div className="hours">
            {[...Array(24)].map((_, h) => (
              <div key={h} className="hour-label" style={{ top: h * HOUR_PX }}>
                {h === 0 ? "" : `${String(h).padStart(2, "0")}:00`}
              </div>
            ))}
          </div>
          {days.map((d, i) => (
            <div key={d.toString()} className={`day-col ${d.equals(today) ? "today" : ""}`}>
              {[...Array(24)].map((_, h) => (
                <div key={h} className="hour-line" style={{ top: h * HOUR_PX }} />
              ))}
              {timed[i].map((b) => (
                <Block
                  key={`${b.event.calendar}/${b.event.id}/${b.event.startsAt}`}
                  block={b}
                  color={colorOf(calendars, b.event)}
                  selected={same(selected, b.event)}
                  onSelect={onSelect}
                />
              ))}
              {d.equals(today) && <div className="now" style={{ top: (minutesInto(d, now) / 60) * HOUR_PX }} />}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function Block({
  block, color, selected, onSelect,
}: { block: Timed; color: string; selected: boolean; onSelect: (e: CalEvent) => void }) {
  const { event, starts, ends } = block;
  const height = ((block.bottom - block.top) / 60) * HOUR_PX;
  const style = {
    "--c": color,
    top: (block.top / 60) * HOUR_PX,
    height: height - 2,
    left: `calc(${(block.column / block.columns) * 100}% + 1px)`,
    width: `calc(${(100 * block.span) / block.columns}% - 3px)`,
  } as React.CSSProperties;
  const span = `${time(starts)}–${time(ends)}`;
  const compact = height < 40;
  const short = height < 64;
  return (
    <button
      className={`block ${tone(event)} ${selected ? "selected" : ""} ${compact ? "compact" : short ? "short" : ""}`}
      style={style}
      onClick={() => onSelect(event)}
      title={`${span} ${event.summary ?? ""}${event.location ? `\n${event.location}` : ""}`}
    >
      <span className="block-title">{event.summary || "(no title)"}</span>
      <span className="block-time">{compact ? time(starts) : span}</span>
      {!compact && event.location && <span className="block-where">{event.location}</span>}
    </button>
  );
}
