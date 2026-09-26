import { useState } from "react";
import { Group, UNCATEGORISED, formatKr, formatPct } from "./data";

interface Props {
  title: string;
  groups: Group[];
  /** What the shares are shares of. */
  whole: number;
  selected: string | null;
  onSelect: (key: string | null) => void;
  limit?: number;
}

/**
 * Ranked horizontal bars, one hue. A selection is emphasis: the chosen bar
 * keeps its colour and the rest recede. Every value is printed beside its
 * bar, so nothing depends on hovering.
 */
export function BarList({ title, groups, whole, selected, onSelect, limit = 12 }: Props) {
  const [all, setAll] = useState(false);
  const shown = all ? groups : groups.slice(0, limit);
  const max = groups[0]?.total ?? 0;
  return (
    <section className="card">
      <h2>{title}</h2>
      {groups.length === 0 && <p className="muted">Nothing in this selection.</p>}
      <ul className="bars" role="listbox" aria-label={title}>
        {shown.map((g) => {
          const isSelected = g.key === selected;
          const dim = selected !== null && !isSelected;
          return (
            <li key={g.key}>
              <button
                className={`bar-row${isSelected ? " selected" : ""}${dim ? " dim" : ""}`}
                role="option"
                aria-selected={isSelected}
                onClick={() => onSelect(isSelected ? null : g.key)}
                title={`${g.key}: ${formatKr(g.total)}, ${g.count} transactions`}
              >
                <span className="bar-label">{g.key}</span>
                <span className="bar-track">
                  <span
                    className={`bar-fill${g.key === UNCATEGORISED ? " unknown" : ""}`}
                    style={{ width: `${max > 0 ? (g.total / max) * 100 : 0}%` }}
                  />
                </span>
                <span className="bar-value">{formatKr(g.total)}</span>
                <span className="bar-share">{whole > 0 ? formatPct(g.total / whole) : ""}</span>
              </button>
            </li>
          );
        })}
      </ul>
      {groups.length > limit && (
        <button className="link" onClick={() => setAll(!all)}>
          {all ? "Show fewer" : `Show all ${groups.length}`}
        </button>
      )}
    </section>
  );
}
