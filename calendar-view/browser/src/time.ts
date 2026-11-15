// Days are `Temporal.PlainDate`s; times are shown in the browser's zone.

export const LOCALE = "sv-SE";

export const timeZone = () => Temporal.Now.timeZoneId();
export const today = () => Temporal.Now.plainDateISO();
export const now = () => Temporal.Now.zonedDateTimeISO();

export function monday(d: Temporal.PlainDate): Temporal.PlainDate {
  return d.subtract({ days: d.dayOfWeek - 1 });
}

/** `YYYY-MM-DD`, or calendula's all-day `YYYYMMDD`. */
export function parseDay(s: string): Temporal.PlainDate | null {
  const m = /^(\d{4})-?(\d{2})-?(\d{2})/.exec(s);
  if (!m) return null;
  try {
    return Temporal.PlainDate.from({ year: Number(m[1]), month: Number(m[2]), day: Number(m[3]) }, { overflow: "reject" });
  } catch {
    return null;
  }
}

/** An RFC 3339 instant in the browser's zone. */
export const zoned = (s: string) => Temporal.Instant.from(s).toZonedDateTimeISO(timeZone());

/** Where a day begins, in the browser's zone. */
export const dayStart = (d: Temporal.PlainDate) => d.toZonedDateTime(timeZone());

export const isoWeek = (d: Temporal.PlainDate) => d.weekOfYear ?? 0;

export const time = (t: Temporal.ZonedDateTime) => t.toLocaleString(LOCALE, { hour: "2-digit", minute: "2-digit" });
export const weekday = (d: Temporal.PlainDate) => d.toLocaleString("en-GB", { weekday: "short" });
export const longDay = (d: Temporal.PlainDate) =>
  d.toLocaleString("en-GB", { weekday: "long", day: "numeric", month: "long", year: "numeric" });
const monthName = (d: Temporal.PlainDate) => d.toLocaleString("en-GB", { month: "long" });

/** "16–22 November 2026", or across months "30 November – 6 December 2026". */
export function rangeTitle(first: Temporal.PlainDate, last: Temporal.PlainDate): string {
  if (first.equals(last)) return longDay(first);
  if (first.month === last.month && first.year === last.year)
    return `${first.day}–${last.day} ${monthName(last)} ${last.year}`;
  const year = first.year === last.year ? "" : ` ${first.year}`;
  return `${first.day} ${monthName(first)}${year} – ${last.day} ${monthName(last)} ${last.year}`;
}
