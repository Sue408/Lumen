import test from "node:test";
import assert from "node:assert/strict";
import type { SessionSummary } from "../../services/usage.ts";
import { formatCurrency } from "../../lib/format.ts";
import { formatSessionLabel, formatSessionTime, shortSessionId } from "./sessionLabel.ts";

function summary(overrides: Partial<SessionSummary> = {}): SessionSummary {
  return {
    sessionId: "ses_ab12cd34ef",
    virtualKeyId: "k1",
    firstSeen: new Date(2026, 8, 12, 14, 3).toISOString(),
    lastSeen: new Date(2026, 8, 12, 15, 0).toISOString(),
    requests: 12,
    totalTokens: 4200,
    cost: 0.34,
    models: ["GPT-5"],
    ...overrides,
  };
}

test("shortSessionId truncates long ids and keeps short ones", () => {
  assert.equal(shortSessionId("ses_ab12cd34ef"), "ses_ab12…");
  assert.equal(shortSessionId("ses_ab12"), "ses_ab12");
  assert.equal(shortSessionId("x"), "x");
});

test("formatSessionTime renders MM-DD HH:mm locally", () => {
  assert.equal(formatSessionTime(new Date(2026, 8, 12, 14, 3).toISOString()), "09-12 14:03");
  assert.equal(formatSessionTime("not-a-date"), "—");
});

test("formatSessionLabel composes the descriptive label", () => {
  assert.equal(
    formatSessionLabel(summary()),
    `ses_ab12… · 09-12 14:03 · 12 次 · ${formatCurrency(0.34)}`,
  );
});

test("formatSessionLabel handles zero cost and a single request", () => {
  assert.equal(
    formatSessionLabel(summary({ cost: 0, requests: 1 })),
    `ses_ab12… · 09-12 14:03 · 1 次 · ${formatCurrency(0)}`,
  );
});
