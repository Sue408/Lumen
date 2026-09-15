const integer = new Intl.NumberFormat("zh-CN");

/** 吞吐速率：tok/s，按数量级保留不同精度。 */
export function formatRate(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return "0 tok/s";
  if (value >= 1000) return `${(value / 1000).toFixed(1)}k tok/s`;
  if (value >= 100) return `${value.toFixed(0)} tok/s`;
  return `${value.toFixed(1)} tok/s`;
}

const YI = 100_000_000;
const WAN = 10_000;
const WAN_YI = 1_000_000_000_000;

const round1 = (value: number) => Math.round(value * 10) / 10;

/**
 * 数量分级：1 万以下精确到个位，之后按万 / 亿 / 万亿每级一位小数。
 * 每级先四舍五入，若已顶到下一级的整数值就提级——保证 `99_999_999`
 * 显示为 `1.0 亿`，而不是 `10000.0 万`。与后端 `format_compact_count` 同规则。
 */
export function formatCompactCount(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return "0";
  if (value >= WAN_YI) return `${round1(value / WAN_YI).toFixed(1)} 万亿`;
  if (value >= YI) {
    const yi = round1(value / YI);
    return yi >= 10_000
      ? `${round1(value / WAN_YI).toFixed(1)} 万亿`
      : `${yi.toFixed(1)} 亿`;
  }
  if (value >= WAN) {
    const wan = round1(value / WAN);
    return wan >= 10_000 ? `${round1(value / YI).toFixed(1)} 亿` : `${wan.toFixed(1)} 万`;
  }
  return integer.format(value);
}

/** 延迟：毫秒；达到 1000ms 转秒；缺失用破折号而非 0。 */
export function formatLatency(ms: number | null): string {
  if (ms === null || !Number.isFinite(ms)) return "—";
  if (ms >= 1000) return `${(ms / 1000).toFixed(1)} s`;
  return `${Math.round(ms)} ms`;
}

/** 成功率：取整百分比。 */
export function formatPercent(rate: number): string {
  if (!Number.isFinite(rate)) return "—";
  return `${Math.round(rate * 100)}%`;
}

export type ConnectivityState = "live" | "error" | "idle";

/** 成功率低于此值即视为异常（与「健康」判定的唯一阈值）。 */
export const CONNECTIVITY_ERROR_THRESHOLD = 0.9;

/** 连通性状态：冷却或低成功率为 error，无流量为 idle，其余为 live。 */
export function connectivityState(
  connection: { total: number; successRate: number } | null,
  cooling: boolean,
): ConnectivityState {
  if (cooling) return "error";
  if (!connection || connection.total === 0) return "idle";
  if (connection.successRate < CONNECTIVITY_ERROR_THRESHOLD) return "error";
  return "live";
}

/** 连通性状态的人话说法：登记簿里给一眼能懂的结果，数值明细交给 tooltip。 */
const CONNECTIVITY_LABEL: Record<ConnectivityState, string> = {
  live: "连通良好",
  error: "连通异常",
  idle: "暂无流量",
};

export function connectivityLabel(state: ConnectivityState): string {
  return CONNECTIVITY_LABEL[state];
}

/**
 * 登记簿行内的连通性摘要：带上「成功率 / 延迟」标签，避免一串裸数字。
 * 没有任何流量时只说明现状；冷却中始终在末尾点明。
 */
export function connectivitySummary(
  connection: { total: number; successRate: number; avgLatencyMs: number | null } | null,
  cooling: boolean,
): string {
  if (!connection || connection.total === 0) {
    return cooling ? "暂无流量 · 冷却中" : "暂无流量";
  }
  const detail = `成功率 ${formatPercent(connection.successRate)} · 延迟 ${formatLatency(connection.avgLatencyMs)}`;
  return cooling ? `${detail} · 冷却中` : detail;
}

/**
 * 把桶序列映射为一条 sparkline 折线路径（去脚手架：只留线，无轴无网格）。
 * 空序列或非法尺寸返回空串，交由调用方决定是否绘制。
 */
export function buildSparklinePath(
  values: number[],
  width: number,
  height: number,
): string {
  if (values.length === 0 || width <= 0 || height <= 0) return "";
  const max = values.reduce((current, value) => Math.max(current, value), 0);
  const step = values.length > 1 ? width / (values.length - 1) : width;
  return values
    .map((value, index) => {
      const x = index * step;
      const y = max <= 0 ? height : height - (value / max) * height;
      return `${index === 0 ? "M" : "L"} ${x.toFixed(2)} ${y.toFixed(2)}`;
    })
    .join(" ");
}
