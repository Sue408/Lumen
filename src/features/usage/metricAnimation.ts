const clampProgress = (progress: number) => Math.min(Math.max(progress, 0), 1);
const integer = new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 0 });

export function formatAnimatedMetric(target: string, progress: number): string {
  const normalized = target.replace(/,/g, "");
  const targetValue = Number.parseFloat(normalized.replace(/[^\d.-]/g, ""));
  const value = targetValue * clampProgress(progress);

  if (target.trim().startsWith("$")) {
    return `$ ${value.toFixed(2)}`;
  }
  if (target.trim().endsWith("万")) {
    return `${value.toFixed(1)} 万`;
  }
  if (target.trim().endsWith("次")) {
    return `${integer.format(Math.round(value))} 次`;
  }
  return value.toFixed(0);
}

