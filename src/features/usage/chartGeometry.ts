type GradientPart = {
  color: string;
  value: number;
};

const round = (value: number) => Number(value.toFixed(2));

export function buildSmoothPath(
  values: number[],
  width: number,
  height: number,
  maxValue: number,
): string {
  if (values.length === 0) return "";

  const points = values.map((value, index) => ({
    x: values.length === 1 ? width / 2 : (index / (values.length - 1)) * width,
    y: height - (Math.min(Math.max(value, 0), maxValue) / maxValue) * height,
  }));

  if (points.length === 1) {
    return `M ${round(points[0].x)} ${round(points[0].y)}`;
  }

  let path = `M ${round(points[0].x)} ${round(points[0].y)}`;
  for (let index = 0; index < points.length - 1; index += 1) {
    const current = points[index];
    const next = points[index + 1];
    const controlOffset = (next.x - current.x) * 0.42;
    path += ` C ${round(current.x + controlOffset)} ${round(current.y)}, ${round(next.x - controlOffset)} ${round(next.y)}, ${round(next.x)} ${round(next.y)}`;
  }
  return path;
}

export function buildCostGradient(parts: GradientPart[]): string {
  const total = parts.reduce((sum, part) => sum + part.value, 0);
  let cursor = 0;
  const stops = parts.map((part, index) => {
    const start = cursor;
    cursor += total === 0 ? 0 : (part.value / total) * 100;
    const end = index === parts.length - 1 ? 100 : round(cursor);
    return `${part.color} ${round(start)}% ${end}%`;
  });
  return `conic-gradient(${stops.join(", ")})`;
}

export function buildAreaPath(
  values: number[],
  width: number,
  height: number,
  maxValue: number,
): string {
  const line = buildSmoothPath(values, width, height, maxValue);
  if (!line) return "";
  return `${line} L ${round(width)} ${round(height)} L 0 ${round(height)} Z`;
}

export type DonutSegment = {
  length: number;
  offset: number;
};

export function buildDonutSegments(
  values: number[],
  gapPercent: number,
): DonutSegment[] {
  const total = values.reduce((sum, value) => sum + Math.max(value, 0), 0);
  let cursor = 0;
  return values.map((value) => {
    const share = total === 0 ? 0 : (Math.max(value, 0) / total) * 100;
    const segment = {
      length: round(Math.max(share - gapPercent, 0)),
      offset: cursor === 0 ? 0 : -round(cursor),
    };
    cursor += share;
    return segment;
  });
}

