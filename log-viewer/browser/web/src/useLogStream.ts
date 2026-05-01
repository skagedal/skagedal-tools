import { useEffect, useRef, useState } from "react";
import type { LogEntry } from "./types";

interface StreamState {
  entries: LogEntry[];
  done: boolean;
  error: string | null;
}

const initial: StreamState = { entries: [], done: false, error: null };

/**
 * Subscribes to /api/stream and accumulates entries. The reducer-style update
 * keeps appending in O(1) amortised, while the consumer can read entries
 * directly off the returned array.
 */
export function useLogStream(): StreamState {
  const [state, setState] = useState<StreamState>(initial);
  const buffer = useRef<LogEntry[]>([]);
  const flushPending = useRef(false);

  useEffect(() => {
    const es = new EventSource("/api/stream");

    const flush = () => {
      flushPending.current = false;
      if (buffer.current.length === 0) return;
      const next = buffer.current;
      buffer.current = [];
      setState((prev) => ({ ...prev, entries: prev.entries.concat(next) }));
    };

    const scheduleFlush = () => {
      if (flushPending.current) return;
      flushPending.current = true;
      // Coalesce bursts of incoming SSE events into a single React commit so
      // the table doesn't re-render once per line at high throughput.
      requestAnimationFrame(flush);
    };

    es.addEventListener("entry", (ev) => {
      try {
        buffer.current.push(JSON.parse((ev as MessageEvent).data));
        scheduleFlush();
      } catch {
        /* ignore malformed events */
      }
    });

    es.addEventListener("end", () => {
      flush();
      setState((prev) => ({ ...prev, done: true }));
      es.close();
    });

    es.onerror = () => {
      setState((prev) => ({ ...prev, error: "(connection lost)" }));
    };

    return () => {
      es.close();
    };
  }, []);

  return state;
}
