import { Spend, formatKr } from "./data";

interface Props {
  rows: Spend[];
  accounts: string[];
  limit?: number;
}

/** The rows behind whatever is selected, newest first: the table view. */
export function Transactions({ rows, accounts, limit = 300 }: Props) {
  const sorted = [...rows].sort((a, b) => b.t.date.localeCompare(a.t.date));
  const shown = sorted.slice(0, limit);
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
            {shown.map((s, i) => (
              <tr key={i}>
                <td>{s.t.date}</td>
                <td title={s.t.descriptor}>{s.payee}</td>
                <td>{s.category}</td>
                <td>{s.t.tags.join(", ")}</td>
                {accounts.length > 1 && <td>{accounts[s.t.account]}</td>}
                <td className="num">{formatKr(s.amount)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {rows.length > limit && <p className="muted">Showing the newest {limit}.</p>}
    </section>
  );
}
