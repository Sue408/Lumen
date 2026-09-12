import type { LayerTone } from "./usageData";

const toneColor: Record<LayerTone, string> = {
  ochre: "var(--chart-ochre)",
  indigo: "var(--chart-indigo)",
  moss: "var(--chart-moss)",
  yellow: "var(--chart-yellow)",
  ink: "var(--ink-faint)",
};

export function toneFor(tone: LayerTone): string {
  return toneColor[tone] ?? "var(--ink-faint)";
}
