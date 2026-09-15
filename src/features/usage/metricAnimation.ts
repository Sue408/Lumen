const clampProgress = (progress: number) => Math.min(Math.max(progress, 0), 1);
const integer = new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 0 });
const money = new Intl.NumberFormat("zh-CN", {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});

/** 从格式化文本里取回数值（去掉千分位与单位），供补间计算使用。 */
export function parseMetricValue(target: string): number {
  const normalized = target.replace(/,/g, "");
  const parsed = Number.parseFloat(normalized.replace(/[^\d.-]/g, ""));
  return Number.isFinite(parsed) ? parsed : 0;
}

/**
 * 按 `target` 的既有格式渲染一个任意数值，保持各指标的展示单位一致。
 * 分级单位（万 / 亿 / 万亿）与「次」可叠加，故先剥离「次」再看量级；
 * 万亿必须先于亿、亿必须先于万判断，否则会被更短的单位抢先命中。
 * 空格只加在中西文交界：`86 次` 有空格，而 `1.2 万次` 没有。
 */
export function formatMetricValue(target: string, value: number): string {
  const unit = target.trim();
  if (unit.startsWith("$")) {
    return `$${money.format(value)}`;
  }
  const hasCalls = unit.endsWith("次");
  const measure = hasCalls ? unit.slice(0, -1).trimEnd() : unit;
  const calls = hasCalls ? "次" : "";
  if (measure.endsWith("万亿")) return `${value.toFixed(1)} 万亿${calls}`;
  if (measure.endsWith("亿")) return `${value.toFixed(1)} 亿${calls}`;
  if (measure.endsWith("万")) return `${value.toFixed(1)} 万${calls}`;
  if (hasCalls) return `${integer.format(Math.round(value))} 次`;
  return value.toFixed(0);
}

export function formatAnimatedMetric(target: string, progress: number): string {
  return formatMetricValue(target, parseMetricValue(target) * clampProgress(progress));
}
