import type { Quality } from "./usageData";

const pct = (value: number) => `${Math.round(value * 100)}%`;

export function QualityLine({ quality }: { quality: Quality }) {
  return (
    <p className="insight-line quality-line">
      <span className="insight-label">质量</span>
      缓存命中 {pct(quality.cacheHitRate)} · 失败 {pct(quality.errorRate)} · 推理占输出{" "}
      {pct(quality.reasoningShare)}
    </p>
  );
}
