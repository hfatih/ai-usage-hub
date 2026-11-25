import type { UsageTrend } from "../types";

/**
 * A small trend line for one usage metric. `current` (the live reading) extends the line to now,
 * because unchanged readings are only stored every 15 minutes. Percentages use a fixed 0-100
 * scale so a flat 95% does not look like a cliff; other units scale to their own range.
 */
export function Sparkline({ trend, current, percent, label }: { trend?: UsageTrend; current: number | null; percent: boolean; label: string }) {
  const points: Array<[number, number]> = [...(trend?.points ?? [])];
  const now = Math.floor(Date.now() / 1000);
  if (current != null && (points.length === 0 || points[points.length - 1][0] < now)) points.push([now, current]);
  if (points.length < 2) return null;

  const width = 120;
  const height = 28;
  const times = points.map(([time]) => time);
  const values = points.map(([, value]) => value);
  const [start, end] = [Math.min(...times), Math.max(...times)];
  const [low, high] = percent ? [0, 100] : [Math.min(...values), Math.max(...values)];
  const x = (time: number) => (end === start ? width : ((time - start) / (end - start)) * width);
  const y = (value: number) => (high === low ? height / 2 : height - 2 - ((value - low) / (high - low)) * (height - 4));
  const path = points.map(([time, value], index) => `${index ? "L" : "M"}${x(time).toFixed(1)},${y(value).toFixed(1)}`).join(" ");

  return (
    <svg className="usage-sparkline" viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none" role="img" aria-label={label}>
      <path d={`${path} L${width},${height} L0,${height} Z`} className="usage-sparkline-fill" />
      <path d={path} className="usage-sparkline-line" />
    </svg>
  );
}
