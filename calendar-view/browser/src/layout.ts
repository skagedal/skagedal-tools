// Turns occurrences into what the grid draws: all-day bars per day, and timed
// blocks per day, side by side where they overlap.

import { CalEvent } from "./api";
import { addDays, parseDay, startOfDay } from "./dates";

export interface Timed {
  event: CalEvent;
  starts: Date;
  ends: Date;
  /** Minutes from the day's midnight, clipped to the day. */
  top: number;
  bottom: number;
  column: number;
  /** How many columns it spans, from `column` rightwards into free ones. */
  span: number;
  columns: number;
  /** Continues from the day before, or into the day after. */
  clippedStart: boolean;
  clippedEnd: boolean;
}

/** An all-day event's days, first and last included. */
export function allDaySpan(e: CalEvent): [Date, Date] | null {
  const first = parseDay(e.start);
  if (!first) return null;
  const after = e.end ? parseDay(e.end) : null;
  const last = after && after > first ? addDays(after, -1) : first;
  return [first, last];
}

export function bounds(e: CalEvent): [Date, Date] | null {
  if (e.allDay) {
    const span = allDaySpan(e);
    return span && [span[0], addDays(span[1], 1)];
  }
  if (!e.startsAt) return null;
  const starts = new Date(e.startsAt);
  const ends = e.endsAt ? new Date(e.endsAt) : starts;
  return [starts, ends > starts ? ends : starts];
}

export function allDayOn(events: CalEvent[], day: Date): CalEvent[] {
  return events.filter((e) => {
    if (!e.allDay) return false;
    const span = allDaySpan(e);
    return !!span && span[0] <= day && day <= span[1];
  });
}

/** Shorter than this still gets a block tall enough to read. */
const MIN_MINUTES = 20;

export function timedOn(events: CalEvent[], day: Date): Timed[] {
  const dayStart = startOfDay(day).getTime();
  const dayEnd = addDays(day, 1).getTime();
  const blocks: Timed[] = [];
  for (const event of events) {
    if (event.allDay) continue;
    const b = bounds(event);
    if (!b) continue;
    const [starts, ends] = b;
    const s = starts.getTime();
    const e = Math.max(ends.getTime(), s);
    const touches = s < dayEnd && (e > dayStart || (e === s && s >= dayStart));
    if (!touches) continue;
    const top = (Math.max(s, dayStart) - dayStart) / 60_000;
    const bottom = (Math.min(e, dayEnd) - dayStart) / 60_000;
    blocks.push({
      event, starts, ends, top,
      bottom: Math.max(bottom, top + MIN_MINUTES),
      column: 0, span: 1, columns: 1,
      clippedStart: s < dayStart,
      clippedEnd: e > dayEnd,
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
