import assert from "node:assert/strict";
import test from "node:test";
import { protocolLabel } from "../../services/protocol.ts";
import { protocolEndpoints } from "./protocolEndpoints.ts";

test("every protocol has an endpoint hint", () => {
  const covered = protocolEndpoints.map((endpoint) => endpoint.protocol).sort();
  const all = (Object.keys(protocolLabel) as Array<keyof typeof protocolLabel>).sort();
  assert.deepEqual(covered, all);
});

test("gemini hint keeps the model in the URL path", () => {
  const gemini = protocolEndpoints.find((endpoint) => endpoint.protocol === "gemini");
  assert.ok(gemini);
  assert.ok(gemini.path.includes("/v1beta/models/"));
  assert.ok(gemini.path.endsWith(":generateContent"));
});
