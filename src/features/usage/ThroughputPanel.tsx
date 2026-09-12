import { useEffect, useMemo, useState } from "react";
import { buildSparklinePath, formatRate } from "../../lib/telemetry";
import { queryTelemetry, type TelemetrySnapshot } from "../../services/telemetry";
import type { PeriodKey } from "./usageData";

const SPARK_WIDTH = 100;
const SPARK_HEIGHT = 96;

/**
 * 周期吞吐区块：与日 / 周 / 月花费视图同步切换，只有一条去脚手架 sparkline。
 * 日视图为整点桶，周 / 月视图为整天桶，纵值是各桶的 tokens/s。
 */
export function ThroughputPanel({
  title,
  periodKey,
  anchor,
  revision,
}: {
  title: string;
  periodKey: PeriodKey;
  anchor: Date;
  revision: number;
}) {
  const [data, setData] = useState<TelemetrySnapshot | null>(null);
  const [failed, setFailed] = useState(false);
  const anchorTime = anchor.getTime();

  useEffect(() => {
    let alive = true;
    queryTelemetry(periodKey, new Date(anchorTime))
      .then((next) => {
        if (!alive) return;
        setData(next);
        setFailed(false);
      })
      .catch(() => {
        if (alive) setFailed(true);
      });
    return () => {
      alive = false;
    };
  }, [periodKey, anchorTime, revision]);

  const path = useMemo(() => {
    if (!data) return "";
    return buildSparklinePath(
      data.throughput.buckets.map((bucket) => bucket.tokensPerSec),
      SPARK_WIDTH,
      SPARK_HEIGHT,
    );
  }, [data]);

  if (failed) {
    return (
      <article className="throughput-panel">
        <p className="throughput-empty">暂时无法读取实时吞吐。</p>
      </article>
    );
  }

  if (!data) {
    return (
      <article className="throughput-panel" aria-busy="true">
        <p className="throughput-empty">正在汇总吞吐…</p>
      </article>
    );
  }

  const { throughput } = data;

  return (
    <article className="throughput-panel" aria-label={title}>
      <header className="throughput-head">
        <h2>{title}</h2>
      </header>
      <div className="throughput-body">
        <div className="throughput-figure">
          <strong>{formatRate(throughput.tokensPerSec)}</strong>
          <span>周期平均 · 峰值 {formatRate(throughput.peakTokensPerSec)}</span>
        </div>
        <svg
          className="throughput-spark"
          viewBox={`0 0 ${SPARK_WIDTH} ${SPARK_HEIGHT}`}
          preserveAspectRatio="none"
          role="img"
          aria-label={`${title}曲线`}
        >
          <path d={path} vectorEffect="non-scaling-stroke" />
        </svg>
      </div>
    </article>
  );
}
