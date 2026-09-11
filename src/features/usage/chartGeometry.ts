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

function xAt(index: number, count: number, width: number): number {
  return count === 1 ? width / 2 : (index / (count - 1)) * width;
}

function yAt(value: number, height: number, maxValue: number): number {
  const safeMax = maxValue > 0 ? maxValue : 1;
  return height - (Math.min(Math.max(value, 0), safeMax) / safeMax) * height;
}

export type StackedArea = {
  name: string;
  tone: string;
  amount: number;
  path: string;
};

/** 累积堆叠面积：第 n 层的下边界即前 n-1 层的累积和。 */
export function stackedAreaPaths(
  layers: StackedLayer[],
  width: number,
  height: number,
  maxValue: number,
): StackedArea[] {
  const count = layers[0]?.values.length ?? 0;
  if (layers.length === 0 || count === 0) return [];

  const lower = new Array<number>(count).fill(0);
  const areas: StackedArea[] = [];
  for (const layer of layers) {
    const upper = lower.map((base, index) => base + (layer.values[index] ?? 0));
    const points: string[] = [];
    for (let index = 0; index < count; index += 1) {
      points.push(`${round(xAt(index, count, width))} ${round(yAt(upper[index], height, maxValue))}`);
    }
    for (let index = count - 1; index >= 0; index -= 1) {
      points.push(`${round(xAt(index, count, width))} ${round(yAt(lower[index], height, maxValue))}`);
    }
    areas.push({
      name: layer.name,
      tone: layer.tone,
      amount: layer.amount,
      path: `M ${points.join(" L ")} Z`,
    });
    for (let index = 0; index < count; index += 1) lower[index] = upper[index];
  }
  return areas;
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

export type LabelAnchor = {
  name: string;
  tone: string;
  amount: number;
  y: number;
};

/** 直接标注锚点：取每层上边界的末端，向下推开并夹在图表高度内。 */
export function labelAnchors(
  layers: StackedLayer[],
  height: number,
  maxValue: number,
  minGap: number,
): LabelAnchor[] {
  const count = layers[0]?.values.length ?? 0;
  if (layers.length === 0 || count === 0) return [];

  let running = 0;
  const anchors: LabelAnchor[] = layers.map((layer) => {
    running += layer.values[count - 1] ?? 0;
    return {
      name: layer.name,
      tone: layer.tone,
      amount: layer.amount,
      y: yAt(running, height, maxValue),
    };
  });

  anchors.sort((a, b) => a.y - b.y);
  for (let index = 1; index < anchors.length; index += 1) {
    anchors[index].y = Math.max(anchors[index].y, anchors[index - 1].y + minGap);
  }
  const overflow = anchors[anchors.length - 1].y - (height - minGap / 2);
  if (overflow > 0) {
    for (const anchor of anchors) anchor.y -= overflow;
  }
  const underflow = anchors[0].y - minGap / 2;
  if (underflow < 0) {
    for (const anchor of anchors) anchor.y -= underflow;
  }
  return anchors.map((anchor) => ({
    ...anchor,
    y: Math.min(Math.max(anchor.y, 0), height),
  }));
}

