export const toneColor: Record<string, string> = {
  ochre: "var(--chart-ochre)",
  indigo: "var(--chart-indigo)",
  moss: "var(--chart-moss)",
  yellow: "var(--chart-yellow)",
  ink: "var(--ink-faint)",
};

export function toneFor(tone: string): string {
  return toneColor[tone] ?? "var(--ink-faint)";
}
