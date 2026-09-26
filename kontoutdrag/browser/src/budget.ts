// Budgets as `/api/data` carries them: each month's lines already filled in
// by the server (src/budget.rs), and what the Budget tab makes of them.

import { topOf } from "./data";

/** One planned amount in a budget file, filled in with what happened. */
export interface BudgetLine {
  name: string;
  category: string;
  merchant?: string;
  /** Planned, positive: money out for a row, money in for income. */
  amount: number;
  basis?: string;
  note?: string;
  /** Spent for a row (a refund makes it smaller), received for income. */
  actual: number;
  /** The transactions behind `actual`. */
  keys: string[];
}

export interface BudgetMonth {
  month: string; // YYYY-MM
  file: string;
  income: BudgetLine[];
  rows: BudgetLine[];
  unbudgeted: { actual: number; keys: string[] };
}

export interface Budgets {
  directory: string;
  /** Files that were skipped, or loaded with something odd about them. */
  warnings: string[];
  months: BudgetMonth[];
}

/** Rows under one top-level category, with their subtotals. */
export interface BudgetGroup {
  name: string;
  rows: BudgetLine[];
  amount: number;
  actual: number;
}

/** Planned and actual, added up. */
export const totals = (lines: { amount: number; actual: number }[]) => ({
  amount: lines.reduce((a, l) => a + l.amount, 0),
  actual: lines.reduce((a, l) => a + l.actual, 0),
});

/** Rows grouped by top-level category, groups in the order they first
 * appear in the file. */
export function groupRows(rows: BudgetLine[]): BudgetGroup[] {
  const groups = new Map<string, BudgetLine[]>();
  for (const row of rows) {
    const name = topOf(row.category);
    groups.set(name, [...(groups.get(name) ?? []), row]);
  }
  return [...groups].map(([name, rows]) => ({ name, rows, ...totals(rows) }));
}

/**
 * Several months as one: lines with the same name, category and merchant
 * are added together. What a quarter or a year view would be drawn from;
 * a single month comes through unchanged.
 */
export function combine(months: BudgetMonth[]): BudgetMonth {
  const merge = (lists: BudgetLine[][]): BudgetLine[] => {
    const out = new Map<string, BudgetLine>();
    for (const line of lists.flat()) {
      const id = JSON.stringify([line.name, line.category, line.merchant ?? null]);
      const seen = out.get(id);
      out.set(
        id,
        seen
          ? {
              ...seen,
              amount: seen.amount + line.amount,
              actual: seen.actual + line.actual,
              keys: [...seen.keys, ...line.keys],
            }
          : line,
      );
    }
    return [...out.values()];
  };
  return {
    month: months.map((m) => m.month).join(", "),
    file: months.map((m) => m.file).join(", "),
    income: merge(months.map((m) => m.income)),
    rows: merge(months.map((m) => m.rows)),
    unbudgeted: {
      actual: months.reduce((a, m) => a + m.unbudgeted.actual, 0),
      keys: months.flatMap((m) => m.unbudgeted.keys),
    },
  };
}

export function localMonth(today: Date): string {
  return `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, "0")}`;
}

/** How much of the months has gone by: all of a past one, none of a
 * future one, and the days so far of the current one. */
export function elapsed(months: string[], today: Date): number {
  const now = localMonth(today);
  let days = 0;
  let gone = 0;
  for (const month of months) {
    const [y, m] = month.split("-").map(Number);
    const length = new Date(y, m, 0).getDate();
    days += length;
    if (month < now) gone += length;
    else if (month === now) gone += today.getDate();
  }
  return days > 0 ? gone / days : 0;
}
