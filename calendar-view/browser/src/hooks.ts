// The page's I/O: the server through react-query, and the URL hash.

import { useCallback, useState, useSyncExternalStore } from "react";
import { keepPreviousData, useMutation, useQueries, useQuery, useQueryClient } from "@tanstack/react-query";
import { useInterval } from "usehooks-ts";
import { askSync, fetchCalendars, fetchEvents, fetchVersion } from "./api";
import { now, parseDay, today } from "./time";

export type Mode = "week" | "day";

export interface Place {
  day: Temporal.PlainDate;
  mode: Mode;
}

const subscribeHash = (onChange: () => void) => {
  window.addEventListener("hashchange", onChange);
  return () => window.removeEventListener("hashchange", onChange);
};

/** `#/2026-11-16/week`, so another page can link to a day. */
export function useHashPlace(): [Place, (place: Place) => void] {
  const hash = useSyncExternalStore(subscribeHash, () => window.location.hash);
  const [, day, mode] = hash.split("/");
  const place: Place = {
    day: (day && parseDay(day)) || today(),
    mode: mode === "day" ? "day" : "week",
  };
  const go = useCallback((p: Place) => {
    window.location.hash = `/${p.day.toString()}/${p.mode}`;
  }, []);
  return [place, go];
}

/** The current time, a minute at a time. */
export function useNow(): Temporal.ZonedDateTime {
  const [t, setT] = useState(now);
  useInterval(() => setT(now()), 60_000);
  return t;
}

export const useCalendars = () =>
  useQuery({ queryKey: ["calendars"], queryFn: fetchCalendars, staleTime: Infinity });

const POLL_MS = 10_000;
const POLL_WHILE_SYNCING_MS = 2_000;

/** The store's version and the sync's status, polled. */
export const useVersion = () =>
  useQuery({
    queryKey: ["version"],
    queryFn: fetchVersion,
    refetchInterval: (q) => (q.state.data?.sync.running ? POLL_WHILE_SYNCING_MS : POLL_MS),
  });

export function useSync() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: askSync,
    onSuccess: () => client.invalidateQueries({ queryKey: ["version"] }),
  });
}

const eventsQuery = (first: Temporal.PlainDate, which: number[], version: string | undefined) => {
  const last = first.add({ days: 6 });
  return {
    queryKey: ["events", first.toString(), which, version],
    queryFn: () => fetchEvents(first.toString(), last.toString(), which),
    staleTime: Infinity,
  };
};

/**
 * The week from `first`, of the calendars at `which`, read again whenever the
 * store's version changes. The weeks either side are read too, so the server
 * has them ready.
 */
export function useWeek(first: Temporal.PlainDate, which: number[], ready: boolean) {
  const version = useVersion().data?.version;
  const enabled = ready && version !== undefined;
  useQueries({
    queries: [-7, 7].map((days) => ({ ...eventsQuery(first.add({ days }), which, version), enabled })),
  });
  return useQuery({ ...eventsQuery(first, which, version), enabled, placeholderData: keepPreviousData });
}
