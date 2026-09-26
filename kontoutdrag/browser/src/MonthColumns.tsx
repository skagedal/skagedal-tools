import { useEffect, useRef, useState } from "react";
import { formatKr } from "./data";

interface Props {
  title: string;
  months: string[];
  totals: number[];
}

const HEIGHT = 240;
// The right margin holds the average's label, clear of the columns.
const PAD = { top: 16, right: 120, bottom: 28, left: 64 };

/** Round up to a clean tick step: 1, 2 or 5 times a power of ten. */
function niceStep(max: number, ticks: number): number {
  const raw = max / ticks || 1;
  const pow = 10 ** Math.floor(Math.log10(raw));
  const unit = raw / pow;
  return (unit <= 1 ? 1 : unit <= 2 ? 2 : unit <= 5 ? 5 : 10) * pow;
}

/** Spending per month as columns, with the period's average as a hairline. */
export function MonthColumns({ title, months, totals }: Props) {
  const [hover, setHover] = useState<number | null>(null);
  const wrap = useRef<HTMLDivElement>(null);
  const [WIDTH, setWidth] = useState(760);
  useEffect(() => {
    const el = wrap.current;
    if (!el) return;
    const observer = new ResizeObserver(([entry]) => setWidth(Math.max(320, entry.contentRect.width)));
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
  const plotW = WIDTH - PAD.left - PAD.right;
  const plotH = HEIGHT - PAD.top - PAD.bottom;
  const step = niceStep(Math.max(...totals, 0), 4);
  const top = step * Math.max(1, Math.ceil(Math.max(...totals, 0) / step));
  const y = (v: number) => PAD.top + plotH - (v / top) * plotH;
  const band = plotW / Math.max(months.length, 1);
  const barW = Math.min(24, band - 2);
  const average = totals.length ? totals.reduce((a, b) => a + b, 0) / totals.length : 0;
  const ticks = Array.from({ length: Math.round(top / step) + 1 }, (_, i) => i * step);
  const labelEvery = Math.ceil(months.length / 12);

  return (
    <section className="card">
      <h2>{title}</h2>
      <div className="chart-wrap" ref={wrap}>
        <svg width={WIDTH} height={HEIGHT} role="img" aria-label={title}>
          {ticks.map((t) => (
            <g key={t}>
              <line className="grid" x1={PAD.left} x2={WIDTH - PAD.right} y1={y(t)} y2={y(t)} />
              <text className="tick" x={PAD.left - 8} y={y(t)} dy="0.32em" textAnchor="end">
                {new Intl.NumberFormat("sv-SE").format(t)}
              </text>
            </g>
          ))}
          {months.map((m, i) => {
            const x = PAD.left + i * band + (band - barW) / 2;
            const h = Math.max(0, y(0) - y(totals[i]));
            const r = Math.min(4, h, barW / 2);
            const top = y(0) - h;
            // Rounded at the data end, square at the baseline.
            const d =
              h > 0
                ? `M${x},${y(0)} V${top + r} Q${x},${top} ${x + r},${top} H${x + barW - r} Q${x + barW},${top} ${x + barW},${top + r} V${y(0)} Z`
                : "";
            return (
              <g
                key={m}
                onPointerEnter={() => setHover(i)}
                onPointerLeave={() => setHover(null)}
                tabIndex={0}
                onFocus={() => setHover(i)}
                onBlur={() => setHover(null)}
                aria-label={`${m}: ${formatKr(totals[i])}`}
              >
                <rect className="hit" x={PAD.left + i * band} y={PAD.top} width={band} height={plotH} />
                {d && <path className={`column${hover === i ? " hover" : ""}`} d={d} />}
                {i % labelEvery === 0 && (
                  <text className="tick" x={x + barW / 2} y={HEIGHT - 8} textAnchor="middle">
                    {m.slice(2)}
                  </text>
                )}
              </g>
            );
          })}
          <line className="baseline" x1={PAD.left} x2={WIDTH - PAD.right} y1={y(0)} y2={y(0)} />
          {average > 0 && (
            <g>
              <line className="average" x1={PAD.left} x2={WIDTH - PAD.right + 6} y1={y(average)} y2={y(average)} />
              <text className="avg-label" x={WIDTH - PAD.right + 10} y={y(average)} dy="-0.2em">
                average
              </text>
              <text className="avg-label strong" x={WIDTH - PAD.right + 10} y={y(average)} dy="1.1em">
                {formatKr(average)}
              </text>
            </g>
          )}
        </svg>
        {hover !== null && (
          <div
            className="tooltip"
            style={{ left: `${PAD.left + (hover + 0.5) * band}px` }}
          >
            <strong>{formatKr(totals[hover])}</strong>
            <span>{months[hover]}</span>
          </div>
        )}
      </div>
    </section>
  );
}
