/** 细密阶梯（1/2/5 太粗会把峰顶压到半高）；峰值再留 5% 顶白，避免贴顶。 */
const AXIS_STEPS = [1, 1.2, 1.5, 2, 2.5, 3, 4, 5, 6, 8];

/** 峰值触到当前上界的这个比例才抬档；其余时间让曲线安静地往上长。 */
export const CEILING_TRIGGER = 0.6;

/** 抬档后让峰值落在绘图高度的这个比例（nice 取整后通常落回 40%~55%）。 */
export const CEILING_TARGET = 0.5;

export function niceMax(value: number): number {
  if (value <= 0) return 10;
  const exponent = Math.floor(Math.log10(value));
  const base = 10 ** exponent;
  const target = (value / base) * 1.05;
  const step = AXIS_STEPS.find((candidate) => target <= candidate) ?? 10;
  return Number((step * base).toPrecision(12));
}

/** 挂载时的初始上界：直接给峰顶留白，让一进来就好看。 */
export function initialCeiling(peak: number): number {
  return niceMax(peak / CEILING_TARGET);
}

/**
 * 迟滞抬档：只有当峰值触到当前上界的 CEILING_TRIGGER 时才重算，否则原样保留。
 * 当天峰值单调不减，所以这是单侧迟滞——只有「抬」，没有「降」，天然不抖。
 */
export function growCeiling(current: number, peak: number): number {
  if (current <= 0) return initialCeiling(peak);
  return peak > current * CEILING_TRIGGER ? initialCeiling(peak) : current;
}
