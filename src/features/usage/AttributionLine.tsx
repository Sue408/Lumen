import type { Attribution } from "./usageData";

const money = new Intl.NumberFormat("zh-CN", {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});

const pct = (value: number) => `${Math.round(value * 100)}%`;

export function AttributionLine({
  attribution,
  previousLabel,
}: {
  attribution: Attribution;
  previousLabel: string;
}) {
  const { deltaCost, topMovers, cache } = attribution;
  const segments: string[] = [];

  if (deltaCost > 0) segments.push(`较${previousLabel}多花 $${money.format(deltaCost)}`);
  else if (deltaCost < 0) segments.push(`较${previousLabel}少花 $${money.format(Math.abs(deltaCost))}`);
  else segments.push(`较${previousLabel}花费持平`);

  const increases = topMovers.filter((mover) => mover.deltaCost > 0);
  const decreases = topMovers.filter((mover) => mover.deltaCost < 0);
  if (increases.length > 0) {
    segments.push(
      `主要来自 ${increases.map((mover) => `${mover.name} +$${money.format(mover.deltaCost)}`).join("、")}`,
    );
  }
  if (decreases.length > 0) {
    segments.push(
      decreases
        .map((mover) => `${mover.name} −$${money.format(Math.abs(mover.deltaCost))}`)
        .join("、"),
    );
  }
  if (cache) {
    segments.push(`缓存命中率 ${pct(cache.fromRate)} → ${pct(cache.toRate)}`);
  }

  return (
    <p className="insight-line attribution-line">
      <span className="insight-label">归因</span>
      {segments.join("；")}
    </p>
  );
}
