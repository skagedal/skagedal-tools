import { Fragment, useEffect, useRef, useState } from "react";
import { Spend, Transaction, categoryOf, formatKr } from "./data";

interface Props {
  rows: Spend[];
  /** Every transaction, unfiltered, for the neighbours of an opened row. */
  all: Transaction[];
  accounts: string[];
  /** Saved comments, by transaction key. */
  comments: Map<string, string>;
  onComment: (key: string, text: string) => Promise<void>;
  limit?: number;
}

/** The rows behind whatever is selected, newest first: the table view. */
export function Transactions({ rows, all, accounts, comments, onComment, limit = 300 }: Props) {
  const [open, setOpen] = useState<Set<string>>(new Set());
  const sorted = [...rows].sort((a, b) => b.t.date.localeCompare(a.t.date));
  const shown = sorted.slice(0, limit);
  const columns = accounts.length > 1 ? 6 : 5;
  const toggle = (key: string) => {
    const next = new Set(open);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    setOpen(next);
  };
  return (
    <section className="card">
      <h2>
        Transactions <span className="muted">{rows.length}</span>
      </h2>
      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Date</th>
              <th>Payee</th>
              <th>Category</th>
              <th>Tags</th>
              {accounts.length > 1 && <th>Account</th>}
              <th className="num">Amount</th>
            </tr>
          </thead>
          <tbody>
            {shown.map((s) => {
              const isOpen = open.has(s.t.key);
              const commented = comments.has(s.t.key);
              return (
                <Fragment key={s.t.key}>
                  <tr
                    className={`row${isOpen ? " open" : ""}`}
                    onClick={() => toggle(s.t.key)}
                    tabIndex={0}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        toggle(s.t.key);
                      }
                    }}
                    aria-expanded={isOpen}
                  >
                    <td>{s.t.date}</td>
                    <td title={s.t.descriptor}>
                      {s.payee}
                      {commented && (
                        <span className="has-comment" title={comments.get(s.t.key)}>
                          {" "}
                          ✎
                        </span>
                      )}
                    </td>
                    <td>{s.category}</td>
                    <td>{s.t.tags.join(", ")}</td>
                    {accounts.length > 1 && <td>{accounts[s.t.account]}</td>}
                    <td className="num">{formatKr(s.amount)}</td>
                  </tr>
                  {isOpen && (
                    <tr className="detail">
                      <td colSpan={columns}>
                        <Detail spend={s} saved={comments.get(s.t.key) ?? ""} onComment={onComment} />
                        <Nearby
                          of={s.t}
                          all={all}
                          accounts={accounts}
                          comments={comments}
                        />
                      </td>
                    </tr>
                  )}
                </Fragment>
              );
            })}
          </tbody>
        </table>
      </div>
      {rows.length > limit && <p className="muted">Showing the newest {limit}.</p>}
    </section>
  );
}

/** How long typing has to pause before a comment is saved. */
const SAVE_AFTER_MS = 700;

function Detail({
  spend,
  saved,
  onComment,
}: {
  spend: Spend;
  saved: string;
  onComment: (key: string, text: string) => Promise<void>;
}) {
  const key = spend.t.key;
  const [text, setText] = useState(saved);
  const [status, setStatus] = useState<"saved" | "pending" | "saving" | "error">("saved");
  const [error, setError] = useState<string | null>(null);
  const timer = useRef<number | undefined>(undefined);
  const latest = useRef(text);
  latest.current = text;

  // Take the saved text when it changes on disk — cleared after a harvest,
  // say — unless there is typing here that has not been saved yet.
  useEffect(() => {
    if (status === "saved") setText(saved);
  }, [saved]);

  const save = async () => {
    window.clearTimeout(timer.current);
    if (latest.current === saved) {
      setStatus("saved");
      return;
    }
    setStatus("saving");
    try {
      await onComment(key, latest.current);
      setStatus("saved");
      setError(null);
    } catch (e) {
      setStatus("error");
      setError(String(e));
    }
  };

  useEffect(() => () => window.clearTimeout(timer.current), []);

  const t = spend.t;
  return (
    <div className="detail-body">
      <textarea
        placeholder="Comments"
        value={text}
        rows={3}
        onChange={(e) => {
          setText(e.target.value);
          setStatus("pending");
          window.clearTimeout(timer.current);
          timer.current = window.setTimeout(save, SAVE_AFTER_MS);
        }}
        onBlur={save}
      />
      <div className="detail-side">
        <span className="muted status">
          {status === "saving" && "Saving…"}
          {status === "pending" && "Editing…"}
          {status === "saved" && (text ? "Saved" : "")}
          {status === "error" && `Not saved: ${error}`}
        </span>
        <a href={calendarUrl(t.date)} target="_blank" rel="noreferrer">
          Calendar on {t.date}
        </a>
        <a href={gmailUrl(t.date)} target="_blank" rel="noreferrer">
          Mail around {t.date}
        </a>
        <span className="muted raw" title="The statement's text field">
          {t.kind} · {t.text}
        </span>
      </div>
    </div>
  );
}

/**
 * Everything booked from the day before to the day after, in every account
 * and whatever the filters say: the repayment, the refund, the other half
 * of a transfer, which the filters above would hide.
 */
function Nearby({
  of,
  all,
  accounts,
  comments,
}: {
  of: Transaction;
  all: Transaction[];
  accounts: string[];
  comments: Map<string, string>;
}) {
  const from = isoDate(shift(of.date, -1));
  const to = isoDate(shift(of.date, 1));
  const near = all
    .filter((t) => t.date >= from && t.date <= to)
    .sort((a, b) => a.date.localeCompare(b.date) || a.amount - b.amount);
  return (
    <div className="nearby">
      <span className="muted">
        {from} – {to}, every account, no filters
      </span>
      <table>
        <tbody>
          {near.map((t) => (
            <tr key={t.key} className={t.key === of.key ? "self" : undefined}>
              <td>{t.date}</td>
              <td title={t.text}>
                {t.merchant || t.descriptor}
                {comments.has(t.key) && (
                  <span className="has-comment" title={comments.get(t.key)}>
                    {" "}
                    ✎
                  </span>
                )}
              </td>
              <td>{categoryOf(t)}</td>
              {accounts.length > 1 && <td>{accounts[t.account]}</td>}
              <td className={`num${t.amount > 0 ? " in" : ""}`}>
                {t.amount > 0 ? "+" : "−"}
                {formatKr(Math.abs(t.amount))}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function isoDate(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

function shift(date: string, days: number): Date {
  const d = new Date(`${date}T12:00:00`);
  d.setDate(d.getDate() + days);
  return d;
}

/** Google Calendar's day view on the booking date. */
function calendarUrl(date: string): string {
  const d = shift(date, 0);
  return `https://calendar.google.com/calendar/r/day/${d.getFullYear()}/${d.getMonth() + 1}/${d.getDate()}`;
}

/**
 * Gmail searching the week around the booking date. A card purchase books
 * a day or two after it is made, so the window leans earlier.
 */
function gmailUrl(date: string): string {
  const fmt = (d: Date) =>
    `${d.getFullYear()}/${String(d.getMonth() + 1).padStart(2, "0")}/${String(d.getDate()).padStart(2, "0")}`;
  const query = `after:${fmt(shift(date, -4))} before:${fmt(shift(date, 3))}`;
  return `https://mail.google.com/mail/u/0/#search/${encodeURIComponent(query)}`;
}
