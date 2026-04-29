import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { LogTable } from "./LogTable";
import { FieldsPopover } from "./FieldsPopover";
import { useLogStream } from "./useLogStream";
import { copyText, stringifyValue } from "./util";
import { entryHaystack, fuzzyMatch } from "./fuzzy";
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
  const [filterText, setFilterText] = useState("");
  const [selectedId, setSelectedId] = useState<number | null>(null);

  const fieldsBtnRef = useRef<HTMLButtonElement>(null);
  const filterInputRef = useRef<HTMLInputElement>(null);

  // Load static meta once.
  useEffect(() => {
    let cancelled = false;
    fetch("/api/meta")
      .then((r) => r.json() as Promise<MetaResponse>)
      .then((meta) => {
        if (cancelled) return;
        setSourceLabel(meta.sourceLabel ?? "");
        // Merge with any fields discovery has already added — entries can
        // arrive over SSE before /api/meta resolves, and we don't want to
        // drop keys we've already seen.
        setFields((prev) => {
          const merged: ManagedField[] = (meta.config.fields ?? []).map((f) => ({
            name: f.name,
            from: [...f.from],
            visible: true,
          }));
          const known = new Set<string>();
          for (const f of merged) for (const k of f.from) known.add(k);
          for (const f of prev) {
            const remaining = f.from.filter((k) => !known.has(k));
            if (remaining.length === 0) continue;
            for (const k of remaining) known.add(k);
            merged.push({ ...f, from: remaining });
          }
          return merged;
        });
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
    const start = processedRef.current;
    if (entries.length === start) return;
    // Capture start in a local — the setFields updater runs during the next
    // render, after we mutate processedRef below, so reading the ref inside
    // the closure would see the post-mutation value and skip the loop.
    processedRef.current = entries.length;
    setFields((prev) => {
      const known = new Set<string>();
      for (const f of prev) for (const k of f.from) known.add(k);
      let next = prev;
      for (let i = start; i < entries.length; i++) {
        for (const key of Object.keys(entries[i]!.data)) {
          if (known.has(key)) continue;
          known.add(key);
          if (next === prev) next = prev.slice();
          next.push({ name: key, from: [key], visible: false });
        }
      }
      return next;
    });
  }, [entries]);

  // The list the user actually sees and navigates. When `filterText` is empty
  // this is just `entries`; otherwise it's filtered by a fuzzy subsequence
  // match against the visible columns of each entry.
  const displayed = useMemo(() => {
    if (filterText.length === 0) return entries;
    return entries.filter((e) => fuzzyMatch(filterText, entryHaystack(e, fields)));
  }, [entries, filterText, fields]);

  // Keep cursor anchored to the same entry across filter changes / arrivals.
  // We track `selectedId`; whenever `displayed` changes shape we re-derive
  // `cursor` from it. In follow mode we ride the tail of `displayed`.
  const [prevTail, setPrevTail] = useState({
    displayed,
    follow,
    entriesLength: entries.length,
  });
  if (
    prevTail.displayed !== displayed ||
    prevTail.follow !== follow ||
    prevTail.entriesLength !== entries.length
  ) {
    setPrevTail({ displayed, follow, entriesLength: entries.length });
    if (follow && displayed.length > 0) {
      const tailIdx = displayed.length - 1;
      if (cursor !== tailIdx) setCursor(tailIdx);
      const tailId = displayed[tailIdx]!.id;
      if (selectedId !== tailId) setSelectedId(tailId);
    } else if (selectedId != null) {
      const idx = displayed.findIndex((e) => e.id === selectedId);
      if (idx >= 0) {
        if (cursor !== idx) setCursor(idx);
      } else if (displayed.length > 0) {
        const clamped = Math.max(0, Math.min(cursor, displayed.length - 1));
        if (cursor !== clamped) setCursor(clamped);
        setSelectedId(displayed[clamped]!.id);
      } else {
        setSelectedId(null);
        if (cursor !== 0) setCursor(0);
      }
    } else if (displayed[cursor]) {
      setSelectedId(displayed[cursor]!.id);
    } else if (displayed.length > 0) {
      setCursor(0);
      setSelectedId(displayed[0]!.id);
    }
  }

  const flash = useCallback((msg: string) => {
    setFlashMsg(msg);
    if (flashTimer.current != null) clearTimeout(flashTimer.current);
    flashTimer.current = window.setTimeout(() => setFlashMsg(null), 1500);
  }, []);

  const moveCursor = useCallback(
    (next: number) => {
      setFollow(false);
      const max = displayed.length - 1;
      if (max < 0) return;
      const clamped = Math.max(0, Math.min(max, next));
      setCursor(clamped);
      setSelectedId(displayed[clamped]!.id);
      setDetailCursor(0);
    },
    [displayed],
  );

  const setSelected = useCallback(
    (next: number) => {
      setFollow(false);
      setCursor(next);
      if (displayed[next]) setSelectedId(displayed[next]!.id);
      setDetailCursor(0);
    },
    [displayed],
  );

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
        const tailIdx = Math.max(0, displayed.length - 1);
        setCursor(tailIdx);
        if (displayed[tailIdx]) setSelectedId(displayed[tailIdx]!.id);
        setDetailCursor(0);
      }
      return next;
    });
  }, [displayed]);

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
      const entry = displayed[cursor];
      const isExpanded = !!entry && expanded.has(entry.id);
      switch (ev.key) {
        case "/":
          filterInputRef.current?.focus();
          filterInputRef.current?.select();
          ev.preventDefault();
          break;
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
          moveCursor(displayed.length - 1);
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
    displayed,
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
    const pos = displayed.length === 0 ? 0 : cursor + 1;
    const eof = done ? " (eof)" : "";
    const tag = follow ? " · FOLLOW" : "";
    const filterTag =
      filterText.length > 0 ? ` · ${displayed.length}/${entries.length} match` : "";
    const base = `${pos}/${displayed.length}${eof}${tag}${filterTag}`;
    if (error) return `${base} · ${error}`;
    if (flashMsg) return `${base} · ${flashMsg}`;
    return base;
  }, [displayed.length, entries.length, cursor, done, follow, error, flashMsg, filterText]);

  const onSelectDetailRow = useCallback(
    (entryIdx: number, detailIdx: number) => {
      setFollow(false);
      setCursor(entryIdx);
      if (displayed[entryIdx]) setSelectedId(displayed[entryIdx]!.id);
      setDetailCursor(detailIdx);
    },
    [displayed],
  );

  return (
    <div id="app">
      <header>
        <strong>log-viewer</strong>
        <span id="meta">{sourceLabel ? `· ${sourceLabel}` : ""}</span>
        <span className="header-spacer" />
        <input
          ref={filterInputRef}
          type="search"
          className="filter-input"
          placeholder="filter (fuzzy)"
          aria-label="Fuzzy filter"
          value={filterText}
          onChange={(ev) => setFilterText(ev.target.value)}
          onKeyDown={(ev) => {
            if (ev.key === "Escape") {
              setFilterText("");
              ev.currentTarget.blur();
              ev.preventDefault();
            } else if (ev.key === "Enter") {
              ev.currentTarget.blur();
              ev.preventDefault();
            }
          }}
        />
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
        entries={displayed}
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
          j/k move · u up · o open · f follow · v fields · g/G top/bottom · / filter · esc close
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
