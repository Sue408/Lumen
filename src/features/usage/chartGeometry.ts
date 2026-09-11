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

export type StackedLayer = {
  name: string;
  tone: string;
  values: number[];
  amount: number;
};

function yAt(value: number, height: number, maxValue: number): number {
  const safeMax = maxValue > 0 ? maxValue : 1;
  return height - (Math.min(Math.max(value, 0), safeMax) / safeMax) * height;
}

export type StackedBarSegment = {
  name: string;
  tone: string;
  x: number;
  y: number;
  width: number;
  height: number;
};

/** 堆叠柱：先把各层累积值差分回当期值，再按桶堆叠。 */
export function stackedBarSegments(
  layers: StackedLayer[],
  bucket: number,
  width: number,
  height: number,
  maxValue: number,
  gapRatio = 0.2,
): StackedBarSegment[] {
  const count = layers[0]?.values.length ?? 0;
  if (layers.length === 0 || count === 0) return [];

  const slot = width / count;
  const gap = slot * gapRatio;
  const barWidth = Math.max(slot - gap, 0);
  const x = bucket * slot + gap / 2;
  const distributed = layers.map((layer) =>
    layer.values.map((value, index) => Math.max(0, value - (layer.values[index - 1] ?? 0))),
  );

  let lower = 0;
  return layers.map((layer, layerIndex) => {
    const upper = lower + (distributed[layerIndex][bucket] ?? 0);
    const yTop = yAt(upper, height, maxValue);
    const yBottom = yAt(lower, height, maxValue);
    const segment: StackedBarSegment = {
      name: layer.name,
      tone: layer.tone,
      x,
      y: yTop,
      width: barWidth,
      height: Math.max(yBottom - yTop, 0),
    };
    lower = upper;
    return segment;
  });
}


