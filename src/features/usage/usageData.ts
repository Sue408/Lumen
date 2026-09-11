export type PeriodKey = "day" | "week" | "month";

export type Metric = {
  label: string;
  value: string;
  comparison: string;
};

export type ChartTone = "ochre" | "indigo" | "moss" | "yellow";

export type LayerTone = ChartTone | "ink";

export type ModelCost = {
  name: string;
  cost: number;
  tone: ChartTone;
};

export type UsageLayer = {
  name: string;
  tone: LayerTone;
  values: number[];
  amount: number;
};

export type Mover = {
  name: string;
  deltaCost: number;
};

export type CacheShift = {
  fromRate: number;
  toRate: number;
};

export type Attribution = {
  deltaCost: number;
  topMovers: Mover[];
  cache: CacheShift | null;
};

export type Quality = {
  cacheHitRate: number;
  errorRate: number;
  reasoningShare: number;
};

export type UsagePeriod = {
  periodKey: PeriodKey;
  heading: string;
  summaryLead: string;
  summaryTail: string;
  metrics: Metric[];
  totalCost: number;
  axisLabels: string[];
  yAxisMax: number;
  series: {
    current: string;
    previous: string;
    currentValues: number[];
    previousValues: number[];
  };
  layers: UsageLayer[];
  attribution: Attribution;
  quality: Quality;
  modelCosts: ModelCost[];
};

export const periodLabels: Record<PeriodKey, string> = {
  day: "日",
  week: "周",
  month: "月",
};

export const usagePeriods: Record<PeriodKey, UsagePeriod> = {
  day: {
    periodKey: "day",
    heading: "今日总账",
    summaryLead: "今天用量平稳",
    summaryTail: "，花费比昨天同期少 8%。",
    metrics: [
      { label: "调用次数", value: "86 次", comparison: "较昨天同期 +4%" },
      { label: "输入 Tokens", value: "48.2 万", comparison: "较昨天同期 +7%" },
      { label: "输出 Tokens", value: "8.6 万", comparison: "较昨天同期 −2%" },
      { label: "总花费", value: "¥ 4.82", comparison: "较昨天同期 −8%" },
    ],
    totalCost: 4.82,
    axisLabels: ["00:00", "06:00", "12:00", "18:00", "现在"],
    yAxisMax: 60,
    series: {
      current: "今天",
      previous: "昨天同期",
      currentValues: [2, 13, 36, 47, 51],
      previousValues: [6, 22, 31, 42, 47],
    },
    layers: [
      { name: "Claude 桌面端", tone: "ochre", values: [0.8, 1.3, 1.8, 2.1, 2.31], amount: 2.31 },
      { name: "GPT-5 脚本", tone: "indigo", values: [0.4, 0.7, 0.95, 1.15, 1.3], amount: 1.3 },
      { name: "手机端", tone: "moss", values: [0.25, 0.4, 0.55, 0.68, 0.77], amount: 0.77 },
      { name: "未归属", tone: "ink", values: [0.15, 0.24, 0.32, 0.39, 0.44], amount: 0.44 },
    ],
    attribution: {
      deltaCost: -0.42,
      topMovers: [
        { name: "Claude 桌面端", deltaCost: 0.31 },
        { name: "未归属", deltaCost: -0.72 },
      ],
      cache: { fromRate: 0.62, toRate: 0.51 },
    },
    quality: { cacheHitRate: 0.51, errorRate: 0.024, reasoningShare: 0.18 },
    modelCosts: [
      { name: "Claude Sonnet", cost: 2.31, tone: "ochre" },
      { name: "GPT-5", cost: 1.3, tone: "indigo" },
      { name: "Gemini Pro", cost: 0.77, tone: "moss" },
      { name: "其他", cost: 0.44, tone: "yellow" },
    ],
  },
  week: {
    periodKey: "week",
    heading: "本周总账",
    summaryLead: "本周调用更频繁",
    summaryTail: "，但单次平均花费下降 11%。",
    metrics: [
      { label: "调用次数", value: "612 次", comparison: "较上周同期 +16%" },
      { label: "输入 Tokens", value: "356.8 万", comparison: "较上周同期 +21%" },
      { label: "输出 Tokens", value: "64.1 万", comparison: "较上周同期 +9%" },
      { label: "总花费", value: "¥ 31.46", comparison: "较上周同期 +7%" },
    ],
    totalCost: 31.46,
    axisLabels: ["周一", "周二", "周三", "周四", "周五", "今天"],
    yAxisMax: 420,
    series: {
      current: "本周",
      previous: "上周同期",
      currentValues: [42, 118, 236, 292, 371, 404],
      previousValues: [58, 146, 214, 268, 329, 362],
    },
    layers: [
      { name: "Claude 桌面端", tone: "ochre", values: [2.1, 4.8, 7.9, 10.6, 13.2, 15.1], amount: 15.1 },
      { name: "GPT-5 脚本", tone: "indigo", values: [1.2, 2.7, 4.4, 6.0, 7.4, 8.49], amount: 8.49 },
      { name: "手机端", tone: "moss", values: [0.7, 1.6, 2.7, 3.6, 4.4, 5.03], amount: 5.03 },
      { name: "未归属", tone: "ink", values: [0.4, 0.9, 1.5, 2.0, 2.4, 2.84], amount: 2.84 },
    ],
    attribution: {
      deltaCost: 2.06,
      topMovers: [
        { name: "Claude 桌面端", deltaCost: 1.8 },
        { name: "手机端", deltaCost: 0.6 },
      ],
      cache: { fromRate: 0.58, toRate: 0.55 },
    },
    quality: { cacheHitRate: 0.55, errorRate: 0.019, reasoningShare: 0.15 },
    modelCosts: [
      { name: "Claude Sonnet", cost: 15.1, tone: "ochre" },
      { name: "GPT-5", cost: 8.49, tone: "indigo" },
      { name: "Gemini Pro", cost: 5.03, tone: "moss" },
      { name: "其他", cost: 2.84, tone: "yellow" },
    ],
  },
  month: {
    periodKey: "month",
    heading: "九月总账",
    summaryLead: "本月用量持续上升",
    summaryTail: "，花费仍低于八月同期。",
    metrics: [
      { label: "调用次数", value: "1,284 次", comparison: "较八月同期 +18%" },
      { label: "输入 Tokens", value: "726.4 万", comparison: "较八月同期 +12%" },
      { label: "输出 Tokens", value: "116.2 万", comparison: "较八月同期 +3%" },
      { label: "总花费", value: "¥ 76.42", comparison: "较八月同期 −6%" },
    ],
    totalCost: 76.42,
    axisLabels: ["1 日", "5 日", "9 日", "13 日", "今天"],
    yAxisMax: 900,
    series: {
      current: "九月",
      previous: "八月同期",
      currentValues: [84, 247, 524, 713, 842],
      previousValues: [122, 318, 486, 662, 751],
    },
    layers: [
      { name: "Claude 桌面端", tone: "ochre", values: [6.2, 14.1, 23.0, 30.5, 36.68], amount: 36.68 },
      { name: "GPT-5 脚本", tone: "indigo", values: [3.5, 7.9, 12.9, 17.1, 20.63], amount: 20.63 },
      { name: "手机端", tone: "moss", values: [2.1, 4.7, 7.7, 10.2, 12.23], amount: 12.23 },
      { name: "未归属", tone: "ink", values: [1.2, 2.6, 4.3, 5.7, 6.88], amount: 6.88 },
    ],
    attribution: {
      deltaCost: -4.88,
      topMovers: [
        { name: "GPT-5 脚本", deltaCost: 2.1 },
        { name: "Claude 桌面端", deltaCost: -6.4 },
      ],
      cache: { fromRate: 0.6, toRate: 0.63 },
    },
    quality: { cacheHitRate: 0.63, errorRate: 0.021, reasoningShare: 0.12 },
    modelCosts: [
      { name: "Claude Sonnet", cost: 36.68, tone: "ochre" },
      { name: "GPT-5", cost: 20.63, tone: "indigo" },
      { name: "Gemini Pro", cost: 12.23, tone: "moss" },
      { name: "其他", cost: 6.88, tone: "yellow" },
    ],
  },
};
