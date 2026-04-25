import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { LogTable } from "./LogTable";
import { FieldsPopover } from "./FieldsPopover";
import { useLogStream } from "./useLogStream";
import { copyText, stringifyValue } from "./util";
import type { ManagedField, MetaResponse } from "./types";

export default function App() {
  const { entries, done, error } = useLogStream();
  const [sourceLabel, setSourceLabel] = useState<string>("");
  const [fields, setFields] = useState<ManagedField[]>([]);
  const [cursor, setCursor] = useState(0);
  const [follow, setFollow] = useState(false);
  const [expanded, setExpanded] = useState<Set<number>>(() => new Set());
  const [detailCursor, setDetailCursor] = useState(0);
  const [fieldsOpen, setFieldsOpen] = useState(false);
  const [flashMsg, setFlashMsg] = useState<string | null>(null);
  const flashTimer = useRef<number | null>(null);

  const fieldsBtnRef = useRef<HTMLButtonElement>(null);

  // Load static meta once.
  useEffect(() => {
    let cancelled = false;
    fetch("/api/meta")
      .then((r) => r.json() as Promise<MetaResponse>)
      .then((meta) => {
        if (cancelled) return;
        setSourceLabel(meta.sourceLabel ?? "");
        const initial: ManagedField[] = (meta.config.fields ?? []).map((f) => ({
          name: f.name,
          from: [...f.from],
          visible: true,
        }));
        setFields(initial);
      })
      .catch(() => {
        /* the SSE error UI will surface failures */
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // Discover new keys from incoming entries and add hidden columns for them.
  const processedRef = useRef(0);
  useEffect(() => {
    if (entries.length === processedRef.current) return;
    setFields((prev) => {
      const known = new Set<string>();
      for (const f of prev) for (const k of f.from) known.add(k);
      let next = prev;
      for (let i = processedRef.current; i < entries.length; i++) {
        for (const key of Object.keys(entries[i]!.data)) {
          if (known.has(key)) continue;
          known.add(key);
          if (next === prev) next = prev.slice();
          next.push({ name: key, from: [key], visible: false });
        }
      }
      return next;
    });
    processedRef.current = entries.length;
  }, [entries]);

  // When the very first entry arrives, snap cursor to 0. In follow mode, keep
  // the cursor pinned to the latest row as new entries land.
  useEffect(() => {
    if (entries.length === 0) return;
    if (follow) {
      setCursor(entries.length - 1);
    } else if (cursor >= entries.length) {
      setCursor(entries.length - 1);
    }
    // We intentionally do not depend on cursor; this effect is about reacting
    // to entry-count changes, not cursor moves.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [entries.length, follow]);

  const flash = useCallback((msg: string) => {
    setFlashMsg(msg);
    if (flashTimer.current != null) clearTimeout(flashTimer.current);
    flashTimer.current = window.setTimeout(() => setFlashMsg(null), 1500);
  }, []);

  const moveCursor = useCallback(
    (next: number) => {
      setFollow(false);
      setCursor((prev) => {
        const max = entries.length - 1;
        if (max < 0) return prev;
        return Math.max(0, Math.min(max, next));
      });
      setDetailCursor(0);
    },
    [entries.length],
  );

  const setSelected = useCallback((next: number) => {
    setFollow(false);
    setCursor(next);
    setDetailCursor(0);
  }, []);

  const toggleExpand = useCallback((id: number) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
    setDetailCursor(0);
  }, []);

  const toggleFollow = useCallback(() => {
    setFollow((f) => {
      const next = !f;
      if (next) {
        setCursor(Math.max(0, entries.length - 1));
        setDetailCursor(0);
      }
      return next;
    });
  }, [entries.length]);

  const toggleByKey = useCallback((key: string) => {
    setFields((prev) => {
      const idx = prev.findIndex((f) => f.from.includes(key));
      if (idx === -1) {
        return [...prev, { name: key, from: [key], visible: true }];
      }
      const next = prev.slice();
      next[idx] = { ...next[idx]!, visible: !next[idx]!.visible };
      return next;
    });
  }, []);

  const toggleFieldVisible = useCallback((index: number) => {
    setFields((prev) => {
      const next = prev.slice();
      next[index] = { ...next[index]!, visible: !next[index]!.visible };
      return next;
    });
  }, []);

  const reorderField = useCallback((from: number, to: number) => {
    setFields((prev) => {
      const next = prev.slice();
      const [item] = next.splice(from, 1);
      if (item) next.splice(to, 0, item);
      return next;
    });
  }, []);

  const openFields = useCallback(() => setFieldsOpen(true), []);
  const closeFields = useCallback(() => setFieldsOpen(false), []);

  // Keyboard handling — mirrors the previous app.js shortcuts.
  useEffect(() => {
    const handler = (ev: KeyboardEvent) => {
      if (ev.metaKey || ev.ctrlKey || ev.altKey) return;
      const target = ev.target as HTMLElement | null;
      if (target && (target.tagName === "INPUT" || target.tagName === "BUTTON")) {
        if (ev.key === "Escape") target.blur?.();
        return;
      }
      if (fieldsOpen) {
        if (ev.key === "Escape" || ev.key === "v" || ev.key === "q") {
          closeFields();
          ev.preventDefault();
        }
        return;
      }
      const entry = entries[cursor];
      const isExpanded = !!entry && expanded.has(entry.id);
      switch (ev.key) {
        case "j":
        case "ArrowDown":
          if (entry && isExpanded) {
            const keys = Object.keys(entry.data);
            if (detailCursor < keys.length) {
              setDetailCursor(detailCursor + 1);
              ev.preventDefault();
              return;
            }
          }
          moveCursor(cursor + 1);
          ev.preventDefault();
          break;
        case "k":
        case "ArrowUp":
          if (isExpanded && detailCursor > 0) {
            setDetailCursor(detailCursor - 1);
            ev.preventDefault();
            return;
          }
          moveCursor(cursor - 1);
          ev.preventDefault();
          break;
        case "u":
          if (entry && isExpanded) {
            toggleExpand(entry.id);
            ev.preventDefault();
            return;
          }
          moveCursor(cursor - 1);
          ev.preventDefault();
          break;
        case "o":
        case "Enter":
          if (entry) {
            setFollow(false);
            toggleExpand(entry.id);
          }
          ev.preventDefault();
          break;
        case "Escape":
          if (entry && isExpanded) {
            toggleExpand(entry.id);
            ev.preventDefault();
          }
          break;
        case "g":
          moveCursor(0);
          ev.preventDefault();
          break;
        case "G":
          moveCursor(entries.length - 1);
          ev.preventDefault();
          break;
        case "f":
          toggleFollow();
          ev.preventDefault();
          break;
        case "v":
          openFields();
          ev.preventDefault();
          break;
        case "t": {
          if (!entry || !isExpanded) return;
          const keys = Object.keys(entry.data);
          if (detailCursor === 0) return;
          const key = keys[detailCursor - 1];
          if (!key) return;
          toggleByKey(key);
          ev.preventDefault();
          break;
        }
        case "c": {
          if (!entry) return;
          if (isExpanded && detailCursor > 0) {
            const keys = Object.keys(entry.data);
            const key = keys[detailCursor - 1];
            if (key) {
              copyText(stringifyValue(entry.data[key])).then((ok) =>
                flash(ok ? `copied ${key}` : "copy failed"),
              );
            }
          } else {
            copyText(JSON.stringify(entry.data, null, 2)).then((ok) =>
              flash(ok ? `copied entry #${entry.id}` : "copy failed"),
            );
          }
          ev.preventDefault();
          break;
        }
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [
    entries,
    cursor,
    detailCursor,
    expanded,
    fieldsOpen,
    moveCursor,
    toggleExpand,
    toggleFollow,
    openFields,
    closeFields,
    toggleByKey,
    flash,
  ]);

  const status = useMemo(() => {
    const pos = entries.length === 0 ? 0 : cursor + 1;
    const eof = done ? " (eof)" : "";
    const tag = follow ? " · FOLLOW" : "";
    const base = `${pos}/${entries.length}${eof}${tag}`;
    if (error) return `${base} · ${error}`;
    if (flashMsg) return `${base} · ${flashMsg}`;
    return base;
  }, [entries.length, cursor, done, follow, error, flashMsg]);

  const onSelectDetailRow = useCallback((entryIdx: number, detailIdx: number) => {
    setFollow(false);
    setCursor(entryIdx);
    setDetailCursor(detailIdx);
  }, []);

  return (
    <div id="app">
      <header>
        <strong>log-viewer</strong>
        <span id="meta">{sourceLabel ? `· ${sourceLabel}` : ""}</span>
        <span className="header-spacer" />
        <button
          ref={fieldsBtnRef}
          type="button"
          aria-haspopup="dialog"
          aria-expanded={fieldsOpen}
          onClick={() => (fieldsOpen ? closeFields() : openFields())}
        >
          Fields
        </button>
      </header>
      <LogTable
        entries={entries}
        fields={fields}
        cursor={cursor}
        follow={follow}
        expanded={expanded}
        detailCursor={detailCursor}
        onSelect={setSelected}
        onToggleExpand={toggleExpand}
        onSelectDetailRow={onSelectDetailRow}
        onToggleByKey={toggleByKey}
        onFlash={flash}
      />
      <footer>
        <span id="status">{status}</span>
        <span className="hint">
          j/k move · u up · o open · f follow · v fields · g/G top/bottom · esc close
        </span>
      </footer>
      {fieldsOpen && (
        <FieldsPopover
          fields={fields}
          anchorRef={fieldsBtnRef}
          onClose={closeFields}
          onToggle={toggleFieldVisible}
          onReorder={reorderField}
        />
      )}
    </div>
  );
}
