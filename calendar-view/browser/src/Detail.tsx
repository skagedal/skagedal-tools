import { CalEvent, Calendar, Person } from "./api";
import { allDaySpan, timedBounds } from "./layout";
import { longDay, time } from "./time";

interface Props {
  event: CalEvent;
  calendar: Calendar | undefined;
  onClose: () => void;
}

function when(e: CalEvent): string {
  if (e.allDay) {
    const span = allDaySpan(e);
    if (!span) return e.start;
    const [first, last] = span;
    return first.equals(last) ? `${longDay(first)}, all day` : `${longDay(first)} – ${longDay(last)}`;
  }
  const b = timedBounds(e);
  if (!b) return "";
  const [s, end] = b;
  const day = s.toPlainDate();
  const endsByMidnight = end.toPlainDate().equals(day) || end.equals(s.add({ days: 1 }).startOfDay());
  if (endsByMidnight) return `${longDay(day)}, ${time(s)}–${time(end)}`;
  return `${longDay(day)} ${time(s)} – ${longDay(end.toPlainDate())} ${time(end)}`;
}

const PARTSTAT: Record<string, string> = {
  ACCEPTED: "accepted",
  DECLINED: "declined",
  TENTATIVE: "maybe",
  "NEEDS-ACTION": "no reply",
};

const who = (p: Person) => p.name || p.email || "?";

/** Splits text so that URLs become links. */
function Linked({ text }: { text: string }) {
  const parts = text.split(/(https?:\/\/[^\s<>"]+)/g);
  return (
    <>
      {parts.map((part, i) =>
        i % 2 ? (
          <a key={i} href={part} target="_blank" rel="noreferrer">
            {part.length > 60 ? `${part.slice(0, 57)}…` : part}
          </a>
        ) : (
          part
        ),
      )}
    </>
  );
}

export function Detail({ event, calendar, onClose }: Props) {
  const people = (event.attendees ?? []).filter((a) => a.cutype !== "RESOURCE" && a.cutype !== "ROOM");
  const description = (event.description ?? "").replace(/_{10,}/g, "").trim();
  return (
    <aside className="detail" aria-label="Event">
      <button className="close" aria-label="Close" onClick={onClose}>×</button>
      <h2>{event.summary || "(no title)"}</h2>
      <p className="when">{when(event)}</p>
      {calendar && (
        <p className="from">
          <span className="swatch" style={{ background: calendar.color }} /> {calendar.label}
          {event.recurring && <span className="muted"> · repeats</span>}
          {event.busyStatus && event.busyStatus !== "BUSY" && (
            <span className="muted"> · {event.busyStatus.toLowerCase()}</span>
          )}
        </p>
      )}
      {event.location && (
        <p>
          <Linked text={event.location} />
        </p>
      )}
      {event.onlineMeetingUrl && (
        <p>
          <a className="join" href={event.onlineMeetingUrl} target="_blank" rel="noreferrer">Join online meeting</a>
        </p>
      )}
      {event.organizer && (event.organizer.name || event.organizer.email) && (
        <p className="muted">Organised by {who(event.organizer)}</p>
      )}
      {people.length > 0 && (
        <details>
          <summary>{people.length} {people.length === 1 ? "guest" : "guests"}</summary>
          <ul className="people">
            {people.map((p, i) => (
              <li key={i}>
                {who(p)}
                {p.partstat && PARTSTAT[p.partstat] && <span className="muted"> · {PARTSTAT[p.partstat]}</span>}
              </li>
            ))}
          </ul>
        </details>
      )}
      {description && (
        <div className="description">
          <Linked text={description} />
        </div>
      )}
    </aside>
  );
}
