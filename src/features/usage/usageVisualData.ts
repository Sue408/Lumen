export type HeatmapCell = {
  day: number | null;
  weekday: number;
  value: number;
  level: number;
  isFuture: boolean;
};

export function cumulativeToDistribution(values: number[]): number[] {
  return values.map((value, index) => Math.max(0, Number((value - (values[index - 1] ?? 0)).toFixed(2))));
}

/**
 * 日用量通常跨一到两个数量级。若按峰值线性等分，除峰值日以外的所有天都会掉进
 * 同一档，整张热力图只剩两三种颜色。这里改成对数等分：非零日内部的数量级差距
 * 决定它们铺多开，0 用量始终独占第 0 档。
 *
 * 非零日之间没有差异时（只有一天有用量，或所有非零日恰好相等）不存在可区分的
 * 中间状态，全部顶到第 4 档——这样它们与 0 日的对比最大。
 */
function intensityScale(values: number[]): (value: number) => number {
  const nonzero = values.filter((value) => value > 0);
  if (nonzero.length === 0) return () => 0;
  const low = Math.log(Math.min(...nonzero));
  const span = Math.log(Math.max(...nonzero)) - low;
  if (span <= 0) return (value) => (value > 0 ? 4 : 0);
  return (value) =>
    value <= 0 ? 0 : Math.min(4, Math.max(1, Math.ceil(((Math.log(value) - low) / span) * 4)));
}

export function buildMonthHeatmap(
  anchor: Date,
  values: number[] = [],
  now = new Date(),
): HeatmapCell[] {
  const year = anchor.getFullYear();
  const month = anchor.getMonth();
  const days = new Date(year, month + 1, 0).getDate();
  const offset = (new Date(year, month, 1).getDay() + 6) % 7;
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const daily = Array.from({ length: days }, (_, index) =>
    Number((values[index] ?? 0).toFixed(2)),
  );
  const intensity = intensityScale(daily);
  const cells: HeatmapCell[] = Array.from({ length: offset }, (_, weekday) => ({
    day: null,
    weekday,
    value: 0,
    level: 0,
    isFuture: false,
  }));
  for (let day = 1; day <= days; day += 1) {
    const date = new Date(year, month, day);
    const isFuture = date.getTime() > today.getTime();
    const value = isFuture ? 0 : daily[day - 1];
    cells.push({ day, weekday: (offset + day - 1) % 7, value, level: intensity(value), isFuture });
  }
  while (cells.length % 7 !== 0) {
    cells.push({ day: null, weekday: cells.length % 7, value: 0, level: 0, isFuture: false });
  }
  return cells;
}

