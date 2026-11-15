// Days are local `Date`s at midnight; times are shown in the browser's zone.

export const LOCALE = "sv-SE";
export const DAY_MS = 86_400_000;

export function startOfDay(d: Date): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate());
}

export function addDays(d: Date, n: number): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate() + n);
}

export function monday(d: Date): Date {
  return addDays(startOfDay(d), -((d.getDay() + 6) % 7));
}

export function sameDay(a: Date, b: Date): boolean {
  return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
}

/** `YYYY-MM-DD`. */
export function iso(d: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** `YYYY-MM-DD` or `YYYYMMDD` as a local day. */
export function parseDay(s: string): Date | null {
  const m = /^(\d{4})-?(\d{2})-?(\d{2})/.exec(s);
  if (!m) return null;
  return new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]));
}

/** ISO 8601 week number. */
export function isoWeek(d: Date): number {
  const t = new Date(Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()));
  t.setUTCDate(t.getUTCDate() + 3 - ((t.getUTCDay() + 6) % 7));
  const firstThursday = new Date(Date.UTC(t.getUTCFullYear(), 0, 4));
  return 1 + Math.round(((t.getTime() - firstThursday.getTime()) / DAY_MS - 3 + ((firstThursday.getUTCDay() + 6) % 7)) / 7);
}

export const time = (d: Date) => d.toLocaleTimeString(LOCALE, { hour: "2-digit", minute: "2-digit" });
export const weekday = (d: Date) => d.toLocaleDateString("en-GB", { weekday: "short" });
export const longDay = (d: Date) =>
  d.toLocaleDateString("en-GB", { weekday: "long", day: "numeric", month: "long", year: "numeric" });

/** "16–22 November 2026", or across months "30 November – 6 December 2026". */
export function rangeTitle(first: Date, last: Date): string {
  const month = (d: Date) => d.toLocaleDateString("en-GB", { month: "long" });
  if (sameDay(first, last)) return longDay(first);
  if (first.getMonth() === last.getMonth())
    return `${first.getDate()}–${last.getDate()} ${month(last)} ${last.getFullYear()}`;
  const year = first.getFullYear() === last.getFullYear() ? "" : ` ${first.getFullYear()}`;
  return `${first.getDate()} ${month(first)}${year} – ${last.getDate()} ${month(last)} ${last.getFullYear()}`;
}
