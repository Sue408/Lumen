import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "../components/tauriRuntime";
import type { PeriodKey } from "../features/usage/usageData";

/** 一个桶的吞吐：日视图为整点桶，周 / 月视图为整天桶。 */
export type ThroughputBucket = {
  at: string;
  tokens: number;
  requests: number;
  tokensPerSec: number;
};

export type Throughput = {
  /** "day" | "week" | "month" */
  period: string;
  totalRequests: number;
  totalTokens: number;
  tokensPerSec: number;
  peakTokensPerSec: number;
  buckets: ThroughputBucket[];
};

/** 单个上游模型的流式生成速度。 */
export type GenerationSpeed = {
  model: string;
  tokensPerSec: number;
  samples: number;
};

/** 被动连通性：近 N 小时的成功率与延迟，只统计有流量的提供商。 */
export type ProviderConnectivity = {
  providerId: string;
  providerName: string | null;
  total: number;
  success: number;
  error: number;
  successRate: number;
  avgLatencyMs: number | null;
  lastSuccessAt: string | null;
  lastErrorAt: string | null;
};

export type TelemetrySnapshot = {
  throughput: Throughput;
  generation: GenerationSpeed[];
  connectivity: ProviderConnectivity[];
  /** 当前处于降级冷却中的上游模型 id。 */
  cooling: string[];
};

export type ProbeResult = {
  ok: boolean;
  httpStatus: number | null;
  latencyMs: number;
  model: string;
  error: string | null;
};

const MOCK_GENERATION: GenerationSpeed[] = [
  { model: "GPT-5", tokensPerSec: 68.4, samples: 42 },
  { model: "Claude Sonnet", tokensPerSec: 52.1, samples: 31 },
  { model: "Gemini Pro", tokensPerSec: 91.7, samples: 18 },
];

function mockPeriodStart(period: PeriodKey, base: Date): Date {
  if (period === "day") {
    return new Date(base.getFullYear(), base.getMonth(), base.getDate());
  }
  if (period === "week") {
    const start = new Date(base.getFullYear(), base.getMonth(), base.getDate());
    const offset = (start.getDay() + 6) % 7;
    start.setDate(start.getDate() - offset);
    return start;
  }
  return new Date(base.getFullYear(), base.getMonth(), 1);
}

function buildMockThroughput(period: PeriodKey, anchor?: Date): Throughput {
  const base = anchor ?? new Date();
  const isDay = period === "day";
  const start = mockPeriodStart(period, base);
  const days = new Date(start.getFullYear(), start.getMonth() + 1, 0).getDate();
  const count = isDay ? 24 : period === "week" ? 7 : days;
  const bucketSeconds = isDay ? 3600 : 86_400;
  const bucketMs = bucketSeconds * 1000;
  const buckets: ThroughputBucket[] = [];
  let totalTokens = 0;
  let totalRequests = 0;
  let peak = 0;
  for (let index = 0; index < count; index += 1) {
    const at = new Date(start.getTime() + index * bucketMs);
    const wave = Math.max(0, Math.sin((index / count) * Math.PI));
    const tokens = Math.round(wave * bucketSeconds * 12 + ((index * 137) % 5_000));
    const requests = Math.round(wave * 26 + (index % 4));
    const tokensPerSec = Number((tokens / bucketSeconds).toFixed(4));
    totalTokens += tokens;
    totalRequests += requests;
    peak = Math.max(peak, tokensPerSec);
    buckets.push({ at: at.toISOString(), tokens, requests, tokensPerSec });
  }
  // 与后端一致：分母是「已过时长」——当前周期只算到现在，历史周期用完整周期。
  const now = Date.now();
  const endMs = start.getTime() + count * bucketMs;
  const elapsedSeconds = Math.max(1, (Math.min(now, endMs) - start.getTime()) / 1000);
  return {
    period,
    totalRequests,
    totalTokens,
    tokensPerSec: Number((totalTokens / elapsedSeconds).toFixed(4)),
    peakTokensPerSec: Number(peak.toFixed(4)),
    buckets,
  };
}

function buildMockTelemetry(period: PeriodKey, anchor?: Date): TelemetrySnapshot {
  return {
    throughput: buildMockThroughput(period, anchor),
    generation: MOCK_GENERATION,
    connectivity: [
      {
        providerId: "p-openai",
        providerName: "OpenAI",
        total: 128,
        success: 126,
        error: 2,
        successRate: 0.9844,
        avgLatencyMs: 612.5,
        lastSuccessAt: new Date().toISOString(),
        lastErrorAt: new Date(Date.now() - 5_400_000).toISOString(),
      },
      {
        providerId: "p-anthropic",
        providerName: "Anthropic",
        total: 64,
        success: 60,
        error: 4,
        successRate: 0.9375,
        avgLatencyMs: 884.2,
        lastSuccessAt: new Date().toISOString(),
        lastErrorAt: new Date(Date.now() - 1_800_000).toISOString(),
      },
    ],
    cooling: [],
  };
}

/** `period` 决定吞吐桶粒度（日 = 整点、周 / 月 = 整天），`anchor` 用于历史周期。 */
export async function queryTelemetry(
  period: PeriodKey = "day",
  anchor?: Date,
): Promise<TelemetrySnapshot> {
  if (!isTauriRuntime(window)) return buildMockTelemetry(period, anchor);
  return invoke<TelemetrySnapshot>("query_telemetry_cmd", {
    period,
    anchor: anchor ? anchor.toISOString() : null,
  });
}

export async function testProvider(providerId: string): Promise<ProbeResult> {
  if (!isTauriRuntime(window)) {
    return { ok: true, httpStatus: 200, latencyMs: 180, model: "mock-model", error: null };
  }
  return invoke<ProbeResult>("test_provider_cmd", { providerId });
}
