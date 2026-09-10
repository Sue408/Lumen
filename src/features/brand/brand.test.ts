import test from "node:test";
import assert from "node:assert/strict";
import { detectBrand } from "./brand.ts";

test("detectBrand matches the obvious provider names", () => {
  assert.equal(detectBrand(["DeepSeek"]), "deepseek");
  assert.equal(detectBrand(["Anthropic"]), "anthropic");
  assert.equal(detectBrand(["OpenAI"]), "openai");
  assert.equal(detectBrand(["Moonshot AI"]), "moonshot");
  assert.equal(detectBrand(["智谱"]), "zhipu");
  assert.equal(detectBrand(["Ollama"]), "ollama");
});

test("detectBrand falls back to base url and model ids", () => {
  assert.equal(detectBrand(["local", "https://api.deepseek.com/v1"]), "deepseek");
  assert.equal(detectBrand(["gateway", null, "gpt-4o-mini"]), "openai");
  assert.equal(detectBrand(["proxy", undefined, "claude-3-5-sonnet"]), "anthropic");
  assert.equal(detectBrand(["", "", "qwen-max"]), "qwen");
});

test("detectBrand is specific before generic and returns null otherwise", () => {
  assert.equal(detectBrand(["Google Gemini"]), "gemini");
  assert.equal(detectBrand(["阿里云百炼"]), "alibaba");
  assert.equal(detectBrand(["my self-hosted box"]), null);
  assert.equal(detectBrand([]), null);
});
