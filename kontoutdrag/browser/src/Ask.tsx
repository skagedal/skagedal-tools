// `kontoutdrag ask`: one card per transaction no rule explains, answered
// with a category and a note, and sent back in one go.

import { useMemo, useState } from "react";
import { AnswerToSend, sendAnswers } from "./api";
import { Data, Transaction, formatAmount } from "./data";
import { calendarUrl, gmailUrl } from "./Transactions";

interface Draft {
  category: string;
  comment: string;
}

export function Ask({
  data,
  ask,
  comments,
}: {
  data: Data;
  ask: { since: string; keys: string[] };
  comments: Map<string, string>;
}) {
  const byKey = useMemo(() => new Map(data.transactions.map((t) => [t.key, t])), [data]);
  const questions = ask.keys
    .flatMap((k) => byKey.get(k) ?? [])
    .sort((a, b) => a.date.localeCompare(b.date));
  const categories = useMemo(
    () => [...new Set(data.transactions.map((t) => t.category).filter(Boolean))].sort(),
    [data],
  );
  const [drafts, setDrafts] = useState<Map<string, Draft>>(new Map());
  const [state, setState] = useState<"answering" | "sending" | "sent" | "error">("answering");
  const [error, setError] = useState<string | null>(null);

  const draftOf = (t: Transaction): Draft =>
    drafts.get(t.key) ?? { category: "", comment: comments.get(t.key) ?? "" };
  const update = (t: Transaction, change: Partial<Draft>) =>
    setDrafts(new Map(drafts).set(t.key, { ...draftOf(t), ...change }));

  const answers: AnswerToSend[] = questions
    .map((t) => ({ key: t.key, ...draftOf(t) }))
    .filter((a) => a.category.trim() || a.comment.trim());

  const send = async () => {
    setState("sending");
    try {
      await sendAnswers(answers);
      setState("sent");
    } catch (e) {
      setState("error");
      setError(String(e));
    }
  };

  if (state === "sent") {
    return (
      <div className="page">
        <h1>Sent</h1>
        <p className="muted">
          {answers.length} {answers.length === 1 ? "answer" : "answers"} sent. You can close this
          window.
        </p>
      </div>
    );
  }

  return (
    <div className="page ask">
      <header>
        <h1>What were these?</h1>
        <p className="muted">
          Since {ask.since}: what no rule categorises, and Swishes to people that no mark explains.
          A category and a note are both optional; leave a row blank to skip it.
        </p>
      </header>
      <datalist id="categories">
        {categories.map((c) => (
          <option key={c} value={c} />
        ))}
      </datalist>
      {questions.length === 0 && <p>Nothing to ask.</p>}
      {questions.map((t) => {
        const draft = draftOf(t);
        return (
          <section key={t.key} className="card question">
            <div className="question-head">
              <span className="date">{t.date}</span>
              <span className={t.amount < 0 ? "amount" : "amount in"}>
                {formatAmount(t.amount)}
              </span>
              <strong>{t.merchant || t.descriptor}</strong>
              {t.merchant && <span className="muted">{t.descriptor}</span>}
              <span className="muted">
                {data.accounts[t.account]} · {t.method ?? t.kind}
                {t.category && ` · now ${t.category}`}
              </span>
            </div>
            <div className="question-body">
              <input
                list="categories"
                placeholder={t.category || "Category"}
                value={draft.category}
                onChange={(e) => update(t, { category: e.target.value })}
              />
              <textarea
                rows={2}
                placeholder="What it was"
                value={draft.comment}
                onChange={(e) => update(t, { comment: e.target.value })}
              />
              <div className="detail-side">
                <a href={calendarUrl(t.date)} target="_blank" rel="noreferrer">
                  Calendar on {t.date}
                </a>
                <a href={gmailUrl(t.date)} target="_blank" rel="noreferrer">
                  Mail around {t.date}
                </a>
              </div>
            </div>
          </section>
        );
      })}
      <footer className="ask-footer">
        <button className="send" disabled={state === "sending"} onClick={send}>
          {state === "sending"
            ? "Sending…"
            : `Send ${answers.length} of ${questions.length}`}
        </button>
        {state === "error" && <span className="error">Not sent: {error}</span>}
      </footer>
    </div>
  );
}
