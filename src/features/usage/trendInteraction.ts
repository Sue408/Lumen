export type TrendDetail = {
  label: string;
  currentValue: number;
  previousValue: number;
  difference: number;
  percentage: number | null;
};

export type TrendPeriodKey = "day" | "week" | "month";

export function getNearestPointIndex(
  pointerX: number,
  plotLeft: number,
  plotWidth: number,
  pointCount: number,
): number {
  if (pointCount <= 1 || plotWidth <= 0) return 0;
  const ratio = Math.min(Math.max((pointerX - plotLeft) / plotWidth, 0), 1);
  return Math.round(ratio * (pointCount - 1));
}

export function buildTrendDetail(
  label: string,
  currentValue: number,
  previousValue: number,
): TrendDetail {
  const difference = currentValue - previousValue;
  return {
    label,
    currentValue,
    previousValue,
    difference,
    percentage:
      previousValue === 0 ? null : Math.round((difference / previousValue) * 100),
  };
}

const pad = (value: number) => String(value).padStart(2, "0");
const weekNames = "一二三四五六日";

function getMondayIndex(date: Date): number {
  return (date.getDay() + 6) % 7;
}

function getMonday(date: Date): Date {
  const monday = new Date(date);
  monday.setHours(0, 0, 0, 0);
  monday.setDate(date.getDate() - getMondayIndex(date));
  return monday;
}

function formatTime(totalMinutes: number): string {
  return `${pad(Math.floor(totalMinutes / 60))}:${pad(totalMinutes % 60)}`;
}

function getDayLabels(now: Date, count: number): string[] {
  const endMinutes = now.getHours() * 60 + now.getMinutes();
  return Array.from({ length: count }, (_, index) => {
    const minutes = Math.round((endMinutes * index) / (count - 1));
    return formatTime(minutes);
  });
}

function getWeekLabel(date: Date, now: Date): string {
  const dayIndex = getMondayIndex(date);
  const prefix = date.toDateString() === now.toDateString() ? "今天" : `周${weekNames[dayIndex]}`;
  return `${prefix} ${date.getDate()}日`;
}

function getWeekLabels(now: Date, count: number): string[] {
  const monday = getMonday(now);
  return Array.from({ length: count }, (_, index) => {
    const date = new Date(monday);
    date.setDate(monday.getDate() + index);
    return getWeekLabel(date, now);
  });
}

function getMonthLabels(now: Date, count: number): string[] {
  const today = now.getDate();
  return Array.from({ length: count }, (_, index) => {
    const day = index === count - 1
      ? today
      : 1 + Math.round(((today - 1) * index) / (count - 1));
    return index === count - 1 ? `今天 ${day}日` : `${day}日`;
  });
}

export function getVisiblePointCount(period: TrendPeriodKey, now: Date): number {
  if (period === "day") return 5;
  if (period === "week") return Math.min(6, getMondayIndex(now) + 1);
  return Math.min(5, Math.max(2, Math.ceil(now.getDate() / 3)));
}

/**
 * How many time buckets of the period have actually elapsed — one per hour for
 * a day. A series is clipped to this before it is drawn, otherwise the full
 * period (0→24h) gets stretched across an axis that only spans 0→now and every
 * event is drawn hours too early.
 */
export function getElapsedBucketCount(period: TrendPeriodKey, now: Date): number {
  if (period === "day") return Math.min(24, now.getHours() + 1);
  if (period === "week") return Math.min(7, getMondayIndex(now) + 1);
  return Math.max(1, now.getDate());
}

export function buildPeriodAxisLabels(period: TrendPeriodKey, now: Date): string[] {
  const count = getVisiblePointCount(period, now);
  if (period === "day") return getDayLabels(now, count);
  if (period === "week") return getWeekLabels(now, count);
  return getMonthLabels(now, count);
}

export function buildPeriodSampleLabels(
  period: TrendPeriodKey,
  now: Date,
  count: number,
): string[] {
  if (count <= 0) return [];
  if (period === "day") {
    const endMinutes = now.getHours() * 60 + now.getMinutes();
    return Array.from({ length: count }, (_, index) =>
      formatTime(Math.round((endMinutes * index) / Math.max(count - 1, 1))),
    );
  }

  if (period === "week") {
    const monday = getMonday(now);
    const elapsedMinutes = getMondayIndex(now) * 24 * 60 + now.getHours() * 60 + now.getMinutes();
    return Array.from({ length: count }, (_, index) => {
      const minutes = Math.round((elapsedMinutes * index) / Math.max(count - 1, 1));
      const date = new Date(monday);
      date.setMinutes(minutes);
      return `${getWeekLabel(date, now)} ${formatTime(date.getHours() * 60 + date.getMinutes())}`;
    });
  }

  const today = now.getDate();
  return Array.from({ length: count }, (_, index) => {
    const day = index === count - 1
      ? today
      : 1 + Math.round(((today - 1) * index) / Math.max(count - 1, 1));
    return index === count - 1 ? `今天 ${day}日` : `${day}日`;
  });
}


export function resampleSeries(values: number[], count: number): number[] {
  if (count <= 0 || values.length === 0) return [];
  if (values.length === count) return values;
  if (values.length === 1) return Array.from({ length: count }, () => values[0]);
  return Array.from({ length: count }, (_, index) => {
    const position = (index / Math.max(count - 1, 1)) * (values.length - 1);
    const left = Math.floor(position);
    const right = Math.min(Math.ceil(position), values.length - 1);
    const fraction = position - left;
    return Number((values[left] + (values[right] - values[left]) * fraction).toFixed(2));
  });
}
