import type { Attribution, Mover } from "./usageData";

const money = new Intl.NumberFormat("zh-CN", {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});

function dominantMover(movers: Mover[], deltaCost: number): Mover | undefined {
  if (deltaCost === 0) return undefined;
  return movers
    .filter((mover) => Math.sign(mover.deltaCost) === Math.sign(deltaCost))
    .sort((a, b) => Math.abs(b.deltaCost) - Math.abs(a.deltaCost))[0];
}

export function AttributionLine({
  attribution,
  previousLabel,
}: {
  attribution: Attribution;
  previousLabel: string;
}) {
  const { deltaCost, topMovers } = attribution;
  const direction = deltaCost > 0 ? "多花" : "少花";
  const verb = deltaCost > 0 ? "增加" : "回落";
  const dominant = dominantMover(topMovers, deltaCost);

  let sentence = `较${previousLabel}花费持平`;
  if (deltaCost !== 0) {
    sentence = `较${previousLabel}${direction} $${money.format(Math.abs(deltaCost))}`;
    if (dominant) sentence += `，主要在${dominant.name}${verb}`;
  }
  sentence += "。";

  const detail = topMovers
    .map(
      (mover) =>
        `${mover.name} ${mover.deltaCost >= 0 ? "+" : "−"}$${money.format(Math.abs(mover.deltaCost))}`,
    )
    .join("、");

  return (
    <p
      className="insight-line attribution-line"
      title={detail ? `涨跌明细：${detail}` : undefined}
    >
      <span className="insight-label">归因</span>
      {sentence}
    </p>
  );
}
