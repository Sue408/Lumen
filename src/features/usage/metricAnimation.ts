const clampProgress = (progress: number) => Math.min(Math.max(progress, 0), 1);
const integer = new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 0 });

/** 从格式化文本里取回数值（去掉千分位与单位），供补间计算使用。 */
export function parseMetricValue(target: string): number {
  const normalized = target.replace(/,/g, "");
  const parsed = Number.parseFloat(normalized.replace(/[^\d.-]/g, ""));
  return Number.isFinite(parsed) ? parsed : 0;
}

/** 按 `target` 的既有格式渲染一个任意数值，保持各指标的展示单位一致。 */
export function formatMetricValue(target: string, value: number): string {
  const unit = target.trim();
  if (unit.startsWith("$")) {
    return `$ ${value.toFixed(2)}`;
  }
  if (unit.endsWith("万")) {
    return `${value.toFixed(1)} 万`;
  }
  if (unit.endsWith("次")) {
    return `${integer.format(Math.round(value))} 次`;
  }
  return value.toFixed(0);
}

export function formatAnimatedMetric(target: string, progress: number): string {
  return formatMetricValue(target, parseMetricValue(target) * clampProgress(progress));
}
