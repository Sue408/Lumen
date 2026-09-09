export type PeriodKey = "day" | "week" | "month";

export type Metric = {
  label: string;
  value: string;
  comparison: string;
};

export type ModelCost = {
  name: string;
  cost: number;
  color: string;
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
  modelCosts: ModelCost[];
};

const colors = {
  ochre: "#9b7854",
  indigo: "#607087",
  moss: "#71806a",
  yellow: "#b39a60",
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
    modelCosts: [
      { name: "Claude Sonnet", cost: 2.31, color: colors.ochre },
      { name: "GPT-5", cost: 1.3, color: colors.indigo },
      { name: "Gemini Pro", cost: 0.77, color: colors.moss },
      { name: "其他", cost: 0.44, color: colors.yellow },
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
    modelCosts: [
      { name: "Claude Sonnet", cost: 15.1, color: colors.ochre },
      { name: "GPT-5", cost: 8.49, color: colors.indigo },
      { name: "Gemini Pro", cost: 5.03, color: colors.moss },
      { name: "其他", cost: 2.84, color: colors.yellow },
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
    modelCosts: [
      { name: "Claude Sonnet", cost: 36.68, color: colors.ochre },
      { name: "GPT-5", cost: 20.63, color: colors.indigo },
      { name: "Gemini Pro", cost: 12.23, color: colors.moss },
      { name: "其他", cost: 6.88, color: colors.yellow },
    ],
  },
};


