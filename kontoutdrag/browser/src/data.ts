// The document `/api/data` serves, and the slicing the view does on it.

export interface Transaction {
  /** Stable across reloads; what a comment is filed under. */
  key: string;
  account: number;
  date: string; // YYYY-MM-DD
  amount: number; // negative is money out
  merchant: string;
  category: string;
  tags: string[];
  descriptor: string;
  /** The statement's free-text field, as exported. */
  text: string;
  kind: "card" | "swish" | "plain";
  resolved: boolean;
}

export interface Data {
  accounts: string[];
  transactions: Transaction[];
  /** Where comments are saved, for showing, not for writing. */
  commentsFile: string;
}

export const UNCATEGORISED = "(uncategorised)";

export interface Filters {
  from: string; // YYYY-MM, inclusive
  to: string; // YYYY-MM, inclusive
  accounts: Set<number>;
  notSpending: Set<string>;
}

export interface Selection {
  category: string | null;
  merchant: string | null;
  tag: string | null;
}

export const NO_SELECTION: Selection = { category: null, merchant: null, tag: null };

/** A row of spending: money out, as a positive number. */
export interface Spend {
  t: Transaction;
  amount: number;
  month: string;
  category: string;
  /** The merchant, or the raw descriptor where no table named one. */
  payee: string;
}

export function categoryOf(t: Transaction): string {
  return t.category || UNCATEGORISED;
}

/** Money out, in the chosen months and accounts, in a category that counts. */
export function spending(data: Data, f: Filters): Spend[] {
  const out: Spend[] = [];
  for (const t of data.transactions) {
    if (t.amount >= 0) continue;
    const month = t.date.slice(0, 7);
    if (month < f.from || month > f.to) continue;
    if (!f.accounts.has(t.account)) continue;
    const category = categoryOf(t);
    if (f.notSpending.has(category)) continue;
    out.push({ t, amount: -t.amount, month, category, payee: t.merchant || t.descriptor });
  }
  return out;
}

export function matches(s: Spend, sel: Selection): boolean {
  return (
    (sel.category === null || s.category === sel.category) &&
    (sel.merchant === null || s.payee === sel.merchant) &&
    (sel.tag === null || s.t.tags.includes(sel.tag))
  );
}

export interface Group {
  key: string;
  total: number;
  count: number;
}

/** Totals by key, largest first. A row may count towards several keys. */
export function groupBy(rows: Spend[], keys: (s: Spend) => string[]): Group[] {
  const map = new Map<string, Group>();
  for (const s of rows) {
    for (const key of keys(s)) {
      const g = map.get(key) ?? { key, total: 0, count: 0 };
      g.total += s.amount;
      g.count += 1;
      map.set(key, g);
    }
  }
  return [...map.values()].sort((a, b) => b.total - a.total || a.key.localeCompare(b.key));
}

/** Every month from `from` to `to`, inclusive, so an empty month shows as zero. */
export function monthsBetween(from: string, to: string): string[] {
  const out: string[] = [];
  let [y, m] = from.split("-").map(Number);
  const [ty, tm] = to.split("-").map(Number);
  while (y < ty || (y === ty && m <= tm)) {
    out.push(`${y}-${String(m).padStart(2, "0")}`);
    m += 1;
    if (m > 12) {
      m = 1;
      y += 1;
    }
  }
  return out;
}

export function addMonths(month: string, n: number): string {
  const [y, m] = month.split("-").map(Number);
  const i = y * 12 + (m - 1) + n;
  return `${Math.floor(i / 12)}-${String((i % 12) + 1).padStart(2, "0")}`;
}

const kr = new Intl.NumberFormat("sv-SE", { maximumFractionDigits: 0 });
export const formatKr = (n: number) => `${kr.format(Math.round(n))} kr`;

const pct = new Intl.NumberFormat("sv-SE", { maximumFractionDigits: 1, minimumFractionDigits: 1 });
export const formatPct = (share: number) => `${pct.format(share * 100)} %`;
