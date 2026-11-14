// The server's endpoints; see src/web.rs.

export interface Calendar {
  key: string;
  account: string;
  id: string;
  name: string;
  label: string;
  color: string;
  /** Shown before anything is toggled. */
  shown: boolean;
}

export interface Person {
  email?: string | null;
  name?: string | null;
  partstat?: string | null;
  role?: string | null;
  cutype?: string | null;
}

/** An occurrence as `calendula event list --json` prints it. */
export interface CalEvent {
  calendar: string;
  id: string;
  uid?: string | null;
  recurrenceId?: string | null;
  summary?: string | null;
  description?: string | null;
  location?: string | null;
  allDay: boolean;
  /** `YYYYMMDD` for an all-day event. */
  start: string;
  /** `YYYYMMDD`, the day after the last, for an all-day event. */
  end?: string | null;
  startsAt?: string | null;
  endsAt?: string | null;
  status?: string | null;
  busyStatus?: string | null;
  transparency?: string | null;
  recurring?: boolean;
  organizer?: Person | null;
  attendees?: Person[] | null;
  onlineMeetingUrl?: string | null;
}

export interface Failure {
  calendar: string;
  error: string;
}

export interface Overview {
  from: string;
  to: string;
  events: CalEvent[];
  failures: Failure[];
}

async function get<T>(path: string): Promise<T> {
  const r = await fetch(path, { cache: "no-store" });
  if (!r.ok) {
    const body = await r.json().catch(() => ({}));
    throw new Error(body.error ?? `${path}: HTTP ${r.status}`);
  }
  return r.json();
}

export const fetchCalendars = () => get<Calendar[]>("/api/calendars");

export interface SyncStatus {
  configured: boolean;
  running: boolean;
  lastOk: string | null;
  lastError: string | null;
}

export interface Version {
  /** Changes whenever the events may have. */
  version: string;
  sync: SyncStatus;
}

export const fetchVersion = () => get<Version>("/api/version");

export async function askSync(): Promise<void> {
  const r = await fetch("/api/sync", { method: "POST" });
  if (!r.ok) throw new Error(`/api/sync: HTTP ${r.status}`);
}

/** `from` and `to` are `YYYY-MM-DD`, both included. */
export const fetchEvents = (from: string, to: string, which: number[]) =>
  get<Overview>(`/api/events/${from}/${to}/${which.join(",")}`);
