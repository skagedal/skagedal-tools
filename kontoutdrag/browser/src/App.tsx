import { useEffect, useMemo, useRef, useState } from "react";
import { Version, fetchComments, fetchData, fetchVersion, saveComment } from "./api";
import { BarList } from "./BarList";
import { MonthColumns } from "./MonthColumns";
import { Transactions } from "./Transactions";
import {
  Data,
  Filters,
  NO_SELECTION,
  Selection,
  UNCATEGORISED,
  addMonths,
  categoryOf,
  childOf,
  formatKr,
  formatPct,
  groupBy,
  matches,
  monthsBetween,
  parentOf,
  spending,
  topOf,
} from "./data";

/** Top-level categories that move money rather than spend it, left out by
 * default. `transfer` leaves out `transfer/saving` too. */
const DEFAULT_NOT_SPENDING = ["transfer", "income", "refunds"];

/** The share of spending left uncategorised that counts as good enough. */
const UNCATEGORISED_TARGET = 0.05;

function thisMonth(): string {
  return new Date().toISOString().slice(0, 7);
}

interface Preset {
  label: string;
  range: () => [string, string];
}

function presets(first: string): Preset[] {
  const last = addMonths(thisMonth(), -1);
  const year = thisMonth().slice(0, 4);
  return [
    { label: "Last 12 months", range: () => [addMonths(last, -11), last] },
    { label: "This month", range: () => [thisMonth(), thisMonth()] },
    { label: "Last month", range: () => [last, last] },
    { label: "Last 3 months", range: () => [addMonths(last, -2), last] },
    { label: "This year", range: () => [`${year}-01`, thisMonth()] },
    { label: "Last year", range: () => [`${Number(year) - 1}-01`, `${Number(year) - 1}-12`] },
    { label: "Everything", range: () => [first, thisMonth()] },
  ];
}

/** How often to ask whether the rules or the comments changed on disk. */
const POLL_MS = 2000;

export function App() {
  const [data, setData] = useState<Data | null>(null);
  const [comments, setComments] = useState<Map<string, string>>(new Map());
  const [problem, setProblem] = useState<string | null>(null);
  const seen = useRef<Version | null>(null);

  // Poll for changes: a rule edited, a statement synced, the comments file
  // cleared by whoever harvested it. Only what changed is fetched again,
  // and the filters and selection stay as they are.
  useEffect(() => {
    let stopped = false;
    const tick = async () => {
      try {
        const version = await fetchVersion();
        if (stopped) return;
        const previous = seen.current;
        seen.current = version;
        setProblem(version.error ? `The rules on disk did not load: ${version.error}` : null);
        if (!previous || previous.data !== version.data) setData(await fetchData());
        if (!previous || previous.comments !== version.comments) setComments(await fetchComments());
      } catch (e) {
        if (!stopped) setProblem(`Lost contact with kontoutdrag: ${e}`);
      }
    };
    tick();
    const id = window.setInterval(tick, POLL_MS);
    return () => {
      stopped = true;
      window.clearInterval(id);
    };
  }, []);

  const onComment = async (key: string, text: string) => {
    await saveComment(key, text);
    setComments(await fetchComments());
  };

  if (!data) {
    return <p className={`page ${problem ? "error" : "muted"}`}>{problem ?? "Loading…"}</p>;
  }
  return (
    <>
      {problem && <p className="banner">{problem}</p>}
      <View data={data} comments={comments} onComment={onComment} />
    </>
  );
}

interface ViewProps {
  data: Data;
  comments: Map<string, string>;
  onComment: (key: string, text: string) => Promise<void>;
}

function View({ data, comments, onComment }: ViewProps) {
  const first = useMemo(
    () => data.transactions.reduce((m, t) => (t.date < m ? t.date : m), "9999").slice(0, 7),
    [data],
  );
  const allPresets = useMemo(() => presets(first), [first]);
  const [preset, setPreset] = useState(0);
  const [range, setRange] = useState<[string, string]>(allPresets[0].range());
  const [accounts, setAccounts] = useState(() => new Set(data.accounts.map((_, i) => i)));
  const [notSpending, setNotSpending] = useState(() => new Set(DEFAULT_NOT_SPENDING));
  // `?category=…&merchant=…&tag=…` opens the view on a selection.
  const [selection, setSelection] = useState<Selection>(() => {
    const q = new URLSearchParams(window.location.search);
    return { category: q.get("category"), merchant: q.get("merchant"), tag: q.get("tag") };
  });

  const categories = useMemo(
    () => [...new Set(data.transactions.map((t) => topOf(categoryOf(t))))].sort(),
    [data],
  );
  const filters: Filters = { from: range[0], to: range[1], accounts, notSpending };
  const rows = useMemo(() => spending(data, filters), [data, range, accounts, notSpending]);
  const months = monthsBetween(range[0], range[1]);
  const total = rows.reduce((a, s) => a + s.amount, 0);
  const uncategorised = rows
    .filter((s) => s.category === UNCATEGORISED)
    .reduce((a, s) => a + s.amount, 0);
  const share = total > 0 ? uncategorised / total : 0;

  // The category list shows one level: the top-level categories, or the
  // children of a selected category that has any, or the siblings of a
  // selected one that has none.
  const hasChildren = (c: string) => rows.some((s) => s.category.startsWith(`${c}/`));
  const level =
    selection.category === null || hasChildren(selection.category)
      ? selection.category
      : parentOf(selection.category);

  // Each list is sliced by the other selections, never by its own, so
  // choosing a category does not collapse the category list to one bar.
  const byCategory = groupBy(
    rows.filter((s) => matches(s, { ...selection, category: level })),
    (s) => [childOf(s.category, level)],
  );
  const levelTotal = byCategory.reduce((a, g) => a + g.total, 0);
  const categoryTotal = rows
    .filter((s) => matches(s, { ...NO_SELECTION, category: selection.category }))
    .reduce((a, s) => a + s.amount, 0);
  const byPayee = groupBy(
    rows.filter((s) => matches(s, { ...selection, merchant: null })),
    (s) => [s.payee],
  );
  const byTag = groupBy(
    rows.filter((s) => matches(s, { ...selection, tag: null })),
    (s) => s.t.tags,
  );
  const selected = rows.filter((s) => matches(s, selection));
  const monthly = months.map((m) =>
    selected.filter((s) => s.month === m).reduce((a, s) => a + s.amount, 0),
  );
  const selectedTotal = selected.reduce((a, s) => a + s.amount, 0);

  const selectionLabel =
    [selection.category, selection.merchant, selection.tag && `#${selection.tag}`]
      .filter(Boolean)
      .join(" · ") || "All spending";

  const toggle = <T,>(set: Set<T>, value: T) => {
    const next = new Set(set);
    if (next.has(value)) next.delete(value);
    else next.add(value);
    return next;
  };

  return (
    <div className="page">
      <header className="filters">
        <label>
          Period
          <select
            value={preset}
            onChange={(e) => {
              const i = Number(e.target.value);
              setPreset(i);
              if (i >= 0) setRange(allPresets[i].range());
            }}
          >
            {allPresets.map((p, i) => (
              <option key={p.label} value={i}>
                {p.label}
              </option>
            ))}
            <option value={-1}>Custom</option>
          </select>
        </label>
        <label>
          From
          <input
            type="month"
            value={range[0]}
            onChange={(e) => {
              setPreset(-1);
              setRange([e.target.value, range[1]]);
            }}
          />
        </label>
        <label>
          To
          <input
            type="month"
            value={range[1]}
            onChange={(e) => {
              setPreset(-1);
              setRange([range[0], e.target.value]);
            }}
          />
        </label>
        {data.accounts.length > 1 && (
          <fieldset>
            <legend>Accounts</legend>
            {data.accounts.map((name, i) => (
              <label key={name} className="check">
                <input
                  type="checkbox"
                  checked={accounts.has(i)}
                  onChange={() => setAccounts(toggle(accounts, i))}
                />
                {name}
              </label>
            ))}
          </fieldset>
        )}
        <details className="not-spending">
          <summary>Not counted as spending: {[...notSpending].sort().join(", ") || "nothing"}</summary>
          <div className="chips">
            {categories.map((c) => (
              <label key={c} className="check">
                <input
                  type="checkbox"
                  checked={notSpending.has(c)}
                  onChange={() => setNotSpending(toggle(notSpending, c))}
                />
                {c}
              </label>
            ))}
          </div>
        </details>
      </header>

      <div className="tiles">
        <div className="tile">
          <span className="tile-label">Spending</span>
          <span className="tile-value">{formatKr(total)}</span>
          <span className="muted">
            {range[0]} – {range[1]}
          </span>
        </div>
        <div className="tile">
          <span className="tile-label">Per month</span>
          <span className="tile-value">{formatKr(total / Math.max(months.length, 1))}</span>
          <span className="muted">over {months.length} months</span>
        </div>
        <button
          className={`tile${selection.category === UNCATEGORISED ? " selected" : ""}`}
          onClick={() =>
            setSelection({
              ...NO_SELECTION,
              category: selection.category === UNCATEGORISED ? null : UNCATEGORISED,
            })
          }
        >
          <span className="tile-label">Uncategorised</span>
          <span className="tile-value">{formatPct(share)}</span>
          <span className="meter" aria-hidden>
            <span
              className={`meter-fill${share <= UNCATEGORISED_TARGET ? " good" : ""}`}
              style={{ width: `${Math.min(share / (UNCATEGORISED_TARGET * 4), 1) * 100}%` }}
            />
            <span className="meter-target" style={{ left: "25%" }} />
          </span>
          <span className="muted">
            {formatKr(uncategorised)} · target {formatPct(UNCATEGORISED_TARGET)}
          </span>
        </button>
      </div>

      <div className="selection">
        <strong>{selectionLabel}</strong>
        {selectionLabel !== "All spending" && (
          <>
            <span className="muted">
              {formatKr(selectedTotal)} · {total > 0 ? formatPct(selectedTotal / total) : ""}
            </span>
            <button className="link" onClick={() => setSelection(NO_SELECTION)}>
              Clear
            </button>
          </>
        )}
      </div>

      <div className="grid">
        <BarList
          title={level ? `Categories in ${level}` : "Categories"}
          groups={byCategory}
          whole={level ? levelTotal : total}
          selected={selection.category === level ? null : selection.category}
          label={(key) => (key === level ? `${key} (itself)` : level ? key.slice(level.length + 1) : key)}
          onSelect={(category) =>
            setSelection({ ...selection, category: category ?? level, merchant: null })
          }
          onUp={
            level === null
              ? undefined
              : () => setSelection({ ...selection, category: parentOf(level), merchant: null })
          }
        />
        <BarList
          title={selection.category ? `Payees in ${selection.category}` : "Payees"}
          groups={byPayee}
          whole={selection.category ? categoryTotal : total}
          selected={selection.merchant}
          onSelect={(merchant) => setSelection({ ...selection, merchant })}
        />
      </div>

      <MonthColumns title={`${selectionLabel} per month`} months={months} totals={monthly} />

      {byTag.length > 0 && (
        <BarList
          title="Tags"
          groups={byTag}
          whole={total}
          selected={selection.tag}
          onSelect={(tag) => setSelection({ ...selection, tag })}
          limit={8}
        />
      )}

      <Transactions
        rows={selected}
        all={data.transactions}
        accounts={data.accounts}
        comments={comments}
        onComment={onComment}
      />
      {comments.size > 0 && (
        <p className="muted footnote">
          ✎ {comments.size} {comments.size === 1 ? "comment" : "comments"} in {data.commentsFile}
        </p>
      )}
    </div>
  );
}
