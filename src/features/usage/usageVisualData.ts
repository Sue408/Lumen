export type HeatmapCell = {
  day: number | null;
  weekday: number;
  value: number;
  level: number;
  isFuture: boolean;
};

export function cumulativeToDistribution(values: number[]) {
  return values.map((value, index) => Math.max(0, Number((value - (values[index - 1] ?? 0)).toFixed(2))));
}

/**
 * 轻核高斯平滑：逐小时用量是脉冲式的，先揉开再插值，尖峰才会连成起伏的波。
 * 边缘用复制延拓，避免首尾被无端压低。仅用于画形，读数仍取原始值。
 */
export function smoothSeries(values: number[], sigma: number): number[] {
  const count = values.length;
  if (count === 0 || sigma <= 0) return values;

  const radius = Math.max(1, Math.ceil(sigma * 3));
  const weights = Array.from({ length: radius * 2 + 1 }, (_, index) =>
    Math.exp(-((index - radius) ** 2) / (2 * sigma * sigma)),
  );
  const weightSum = weights.reduce((sum, weight) => sum + weight, 0);

  return values.map((_, index) => {
    let sum = 0;
    for (let offset = -radius; offset <= radius; offset += 1) {
      const sample = Math.min(Math.max(index + offset, 0), count - 1);
      sum += values[sample] * weights[offset + radius];
    }
    return sum / weightSum;
  });
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
  const peak = values.reduce((max, value) => Math.max(max, value), 0);
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
    const value = isFuture ? 0 : Number((values[day - 1] ?? 0).toFixed(2));
    const level =
      value === 0 || peak === 0
        ? 0
        : Math.min(4, Math.max(1, Math.ceil((value / peak) * 4)));
    cells.push({ day, weekday: (offset + day - 1) % 7, value, level, isFuture });
  }
  while (cells.length % 7 !== 0) {
    cells.push({ day: null, weekday: cells.length % 7, value: 0, level: 0, isFuture: false });
  }
  return cells;
}

