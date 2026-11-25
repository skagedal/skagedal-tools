// Turns occurrences into what the grid draws: all-day bars per day, and timed
// blocks per day, side by side where they overlap.

import { CalEvent } from "./api";
import { dayStart, parseDay, zoned } from "./time";

export interface Timed {
  event: CalEvent;
  starts: Temporal.ZonedDateTime;
  ends: Temporal.ZonedDateTime;
  /** Minutes from the day's start, clipped to the day. */
  top: number;
  bottom: number;
  column: number;
  /** How many columns it spans, from `column` rightwards into free ones. */
  span: number;
  columns: number;
}

/** An all-day event's days, first and last included. */
export function allDaySpan(e: CalEvent): [Temporal.PlainDate, Temporal.PlainDate] | null {
  const first = parseDay(e.start);
  if (!first) return null;
  const after = e.end ? parseDay(e.end) : null;
  const last = after && Temporal.PlainDate.compare(after, first) > 0 ? after.subtract({ days: 1 }) : first;
  return [first, last];
}

/** A timed event's start and end, an end before the start taken as the start. */
export function timedBounds(e: CalEvent): [Temporal.ZonedDateTime, Temporal.ZonedDateTime] | null {
  if (e.allDay || !e.startsAt) return null;
  const starts = zoned(e.startsAt);
  const ends = e.endsAt ? zoned(e.endsAt) : starts;
  return [starts, Temporal.ZonedDateTime.compare(ends, starts) > 0 ? ends : starts];
}

const within = (d: Temporal.PlainDate, [first, last]: [Temporal.PlainDate, Temporal.PlainDate]) =>
  Temporal.PlainDate.compare(first, d) <= 0 && Temporal.PlainDate.compare(d, last) <= 0;

export function allDayOn(events: CalEvent[], day: Temporal.PlainDate): CalEvent[] {
  return events.filter((e) => {
    const span = e.allDay ? allDaySpan(e) : null;
    return !!span && within(day, span);
  });
}

/** Shorter than this still gets a block tall enough to read. */
const MIN_MINUTES = 20;

/** Minutes since the day began; a day with a clock change is not 1440 long. */
export const minutesInto = (day: Temporal.PlainDate, t: Temporal.ZonedDateTime) =>
  (t.epochMilliseconds - dayStart(day).epochMilliseconds) / 60_000;

export function timedOn(events: CalEvent[], day: Temporal.PlainDate): Timed[] {
  const from = dayStart(day).epochMilliseconds;
  const to = dayStart(day.add({ days: 1 })).epochMilliseconds;
  const blocks: Timed[] = [];
  for (const event of events) {
    const b = timedBounds(event);
    if (!b) continue;
    const [starts, ends] = b;
    const s = starts.epochMilliseconds;
    const e = ends.epochMilliseconds;
    const touches = s < to && (e > from || (e === s && s >= from));
    if (!touches) continue;
    const top = (Math.max(s, from) - from) / 60_000;
    const bottom = (Math.min(e, to) - from) / 60_000;
    blocks.push({
      event, starts, ends, top,
      bottom: Math.max(bottom, top + MIN_MINUTES),
      column: 0, span: 1, columns: 1,
    });
  }
  blocks.sort((a, b) => a.top - b.top || b.bottom - a.bottom);

  // Clusters of transitively overlapping blocks share their columns.
  let cluster: Timed[] = [];
  let clusterEnd = -1;
  const flush = () => {
    const columnEnds: number[] = [];
    for (const block of cluster) {
      let c = columnEnds.findIndex((end) => end <= block.top);
      if (c < 0) c = columnEnds.push(0) - 1;
      columnEnds[c] = block.bottom;
      block.column = c;
    }
    const overlaps = (a: Timed, b: Timed) => a.top < b.bottom && b.top < a.bottom;
    for (const block of cluster) {
      block.columns = columnEnds.length;
      let span = 1;
      while (
        block.column + span < block.columns &&
        !cluster.some((o) => o.column === block.column + span && overlaps(o, block))
      )
        span++;
      block.span = span;
    }
    cluster = [];
  };
  for (const block of blocks) {
    if (block.top >= clusterEnd) flush();
    cluster.push(block);
    clusterEnd = Math.max(clusterEnd, block.bottom);
  }
  flush();
  return blocks;
}
