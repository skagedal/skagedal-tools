import { useMemo, useState } from "react";
import { Transactions } from "./Transactions";
import {
  BudgetLine,
  BudgetMonth,
  combine,
  elapsed,
  groupRows,
  localMonth,
  totals,
} from "./budget";
import { Data, Spend, Transaction, categoryOf, formatKr, formatPct } from "./data";

interface Props {
  data: Data;
  comments: Map<string, string>;
  onComment: (key: string, text: string) => Promise<void>;
}

/** Which line's transactions are open. */
type Open = { kind: "income" | "row"; index: number } | null;

/** The month a budget view opens on: this one if it has a budget, else
 * the latest before it, else the earliest. */
function initialMonth(months: BudgetMonth[], now: string): string {
  const before = months.filter((m) => m.month <= now);
  return (before.at(-1) ?? months[0]).month;
}

export function BudgetView({ data, comments, onComment }: Props) {
  const budgets = data.budgets;
  const now = localMonth(new Date());
  const [chosen, setChosen] = useState<string | null>(null);
  const [open, setOpen] = useState<Open>(null);
  const byKey = useMemo(() => new Map(data.transactions.map((t) => [t.key, t])), [data]);

  if (!budgets) {
    return (
      <section className="card">
        <h2>No budgets</h2>
        <p className="muted">
          Name a directory of monthly budget files, <code>YYYY-MM.yaml</code>, in settings.toml:
        </p>
        <pre>{`[budgets]\npath = "~/notes/finances/budget"`}</pre>
      </section>
    );
  }
  const warnings = budgets.warnings.length > 0 && (
    <ul className="warnings">
      {budgets.warnings.map((w) => (
        <li key={w}>{w}</li>
      ))}
    </ul>
  );
  if (budgets.months.length === 0) {
    return (
      <>
        {warnings}
        <section className="card">
          <h2>No budgets</h2>
          <p className="muted">
            There are no <code>YYYY-MM.yaml</code> files in {budgets.directory}.
          </p>
        </section>
      </>
    );
  }

  const month =
    budgets.months.find((m) => m.month === chosen)?.month ?? initialMonth(budgets.months, now);
  const budget = combine(budgets.months.filter((m) => m.month === month));
  const pace = elapsed([month], new Date());
  const groups = groupRows(budget.rows);
  const rowTotal = totals(budget.rows);
  const incomeTotal = totals(budget.income);
  const spent = rowTotal.actual + budget.unbudgeted.actual;

  const spends = (keys: string[], sign: number): Spend[] =>
    keys
      .map((k) => byKey.get(k))
      .filter((t): t is Transaction => t !== undefined)
      .map((t) => ({
        t,
        amount: sign * t.amount,
        month: t.date.slice(0, 7),
        category: categoryOf(t),
        payee: t.merchant || t.descriptor,
      }));
  const openLine =
    open && (open.kind === "income" ? budget.income : budget.rows)[open.index];
  const toggle = (next: Open) =>
    setOpen(open && next && open.kind === next.kind && open.index === next.index ? null : next);

  return (
    <>
      {warnings}
      <header className="filters">
        <label>
          Month
          <select
            value={month}
            onChange={(e) => {
              setChosen(e.target.value);
              setOpen(null);
            }}
          >
            {budgets.months.map((m) => (
              <option key={m.month} value={m.month}>
                {m.month}
                {m.month === now ? " (this month)" : ""}
              </option>
            ))}
          </select>
        </label>
        <span className="muted budget-file" title={budget.file}>
          {budget.file}
        </span>
      </header>

      <div className="tiles">
        <div className="tile">
          <span className="tile-label">Spent of budget</span>
          <span className="tile-value">{formatKr(rowTotal.actual)}</span>
          <Progress planned={rowTotal.amount} actual={rowTotal.actual} pace={pace} />
          <span className="muted">
            of {formatKr(rowTotal.amount)} ·{" "}
            <Left planned={rowTotal.amount} actual={rowTotal.actual} suffix=" left" />
          </span>
        </div>
        <div className="tile">
          <span className="tile-label">Income</span>
          <span className="tile-value">{formatKr(incomeTotal.actual)}</span>
          <Progress planned={incomeTotal.amount} actual={incomeTotal.actual} pace={pace} income />
          <span className="muted">of {formatKr(incomeTotal.amount)} planned</span>
        </div>
        <div className="tile">
          <span className="tile-label">Unbudgeted</span>
          <span className="tile-value">{formatKr(budget.unbudgeted.actual)}</span>
          <span className="muted">
            {budget.unbudgeted.keys.length} transactions · all spending {formatKr(spent)}
          </span>
        </div>
        <div className="tile">
          <span className="tile-label">Month gone</span>
          <span className="tile-value">{formatPct(pace)}</span>
          <span className="muted">the marker on each bar</span>
        </div>
      </div>

      <section className="card">
        <table className="budget">
          <thead>
            <tr>
              <th>Line</th>
              <th className="num">Budget</th>
              <th className="num">Actual</th>
              <th className="num">Left</th>
              <th className="progress-col" />
            </tr>
          </thead>
          {budget.income.length > 0 && (
            <tbody>
              <GroupHeader name="income" planned={incomeTotal.amount} actual={incomeTotal.actual} pace={pace} income />
              {budget.income.map((line, i) => (
                <LineRow
                  key={i}
                  line={line}
                  pace={pace}
                  income
                  isOpen={open?.kind === "income" && open.index === i}
                  onClick={() => toggle({ kind: "income", index: i })}
                />
              ))}
            </tbody>
          )}
          {groups.map((g) => (
            <tbody key={g.name}>
              <GroupHeader name={g.name} planned={g.amount} actual={g.actual} pace={pace} />
              {g.rows.map((line) => {
                const i = budget.rows.indexOf(line);
                return (
                  <LineRow
                    key={i}
                    line={line}
                    pace={pace}
                    isOpen={open?.kind === "row" && open.index === i}
                    onClick={() => toggle({ kind: "row", index: i })}
                  />
                );
              })}
            </tbody>
          ))}
          <tfoot>
            <tr className="total">
              <td>Budgeted</td>
              <td className="num">{formatKr(rowTotal.amount)}</td>
              <td className="num">{formatKr(rowTotal.actual)}</td>
              <td className="num">
                <Left planned={rowTotal.amount} actual={rowTotal.actual} />
              </td>
              <td className="progress-col">
                <Progress planned={rowTotal.amount} actual={rowTotal.actual} pace={pace} />
              </td>
            </tr>
            <tr>
              <td>Unbudgeted</td>
              <td className="num" />
              <td className="num">{formatKr(budget.unbudgeted.actual)}</td>
              <td className="num" />
              <td />
            </tr>
            <tr className="total">
              <td>All spending</td>
              <td className="num">{formatKr(rowTotal.amount)}</td>
              <td className="num">{formatKr(spent)}</td>
              <td className="num">
                <Left planned={rowTotal.amount} actual={spent} />
              </td>
              <td className="progress-col">
                <Progress planned={rowTotal.amount} actual={spent} pace={pace} />
              </td>
            </tr>
          </tfoot>
        </table>
      </section>

      {openLine && (
        <Transactions
          title={openLine.name}
          rows={spends(openLine.keys, open?.kind === "income" ? 1 : -1)}
          all={data.transactions}
          accounts={data.accounts}
          comments={comments}
          onComment={onComment}
        />
      )}
      <Transactions
        title="Unbudgeted"
        rows={spends(budget.unbudgeted.keys, -1)}
        all={data.transactions}
        accounts={data.accounts}
        comments={comments}
        onComment={onComment}
      />
    </>
  );
}

function GroupHeader({
  name,
  planned,
  actual,
  pace,
  income = false,
}: {
  name: string;
  planned: number;
  actual: number;
  pace: number;
  income?: boolean;
}) {
  return (
    <tr className="group">
      <th scope="rowgroup">{name}</th>
      <td className="num">{formatKr(planned)}</td>
      <td className="num">{formatKr(actual)}</td>
      <td className="num">
        <Left planned={planned} actual={actual} income={income} />
      </td>
      <td className="progress-col">
        <Progress planned={planned} actual={actual} pace={pace} income={income} />
      </td>
    </tr>
  );
}

function LineRow({
  line,
  pace,
  income = false,
  isOpen,
  onClick,
}: {
  line: BudgetLine;
  pace: number;
  income?: boolean;
  isOpen: boolean;
  onClick: () => void;
}) {
  const detail = [line.category, line.merchant, line.basis].filter(Boolean).join(" · ");
  return (
    <tr
      className={`row${isOpen ? " open" : ""}`}
      onClick={onClick}
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onClick();
        }
      }}
      aria-expanded={isOpen}
      title={line.note}
    >
      <td>
        <span className="line-name">{line.name}</span>
        <span className="muted line-detail">{detail}</span>
      </td>
      <td className="num">{formatKr(line.amount)}</td>
      <td className="num">{formatKr(line.actual)}</td>
      <td className="num">
        <Left planned={line.amount} actual={line.actual} income={income} />
      </td>
      <td className="progress-col">
        <Progress planned={line.amount} actual={line.actual} pace={pace} income={income} />
      </td>
    </tr>
  );
}

/** What is left of a row, or how far over it went. Income has no over:
 * more than planned is only good news. */
function Left({
  planned,
  actual,
  income = false,
  suffix = "",
}: {
  planned: number;
  actual: number;
  income?: boolean;
  suffix?: string;
}) {
  const left = planned - actual;
  if (left >= 0 || income) return <>{formatKr(Math.max(left, 0)) + suffix}</>;
  return <span className="over-text">{formatKr(-left)} over</span>;
}

/**
 * Actual against planned. The track is the budget; past it, the part over
 * is drawn in its own colour and the budget's end stays marked. The thin
 * mark is how far through the month it is, so spending ahead of the month
 * shows as a fill past it.
 */
function Progress({
  planned,
  actual,
  pace,
  income = false,
}: {
  planned: number;
  actual: number;
  pace: number;
  income?: boolean;
}) {
  const scale = Math.max(planned, actual, 0);
  if (scale <= 0) return <span className="progress" aria-hidden />;
  const within = ((income ? Math.max(actual, 0) : Math.min(Math.max(actual, 0), planned)) / scale) * 100;
  const over = !income && actual > planned ? ((actual - planned) / scale) * 100 : 0;
  const end = (planned / scale) * 100;
  return (
    <span
      className="progress"
      role="img"
      aria-label={`${formatKr(actual)} of ${formatKr(planned)}`}
    >
      <span className="progress-budget" style={{ width: `${end}%` }} />
      <span className="progress-fill" style={{ width: `${within}%` }} />
      {over > 0 && <span className="progress-over" style={{ left: `${end}%`, width: `${over}%` }} />}
      {planned > 0 && <span className="progress-pace" style={{ left: `${pace * end}%` }} />}
    </span>
  );
}
