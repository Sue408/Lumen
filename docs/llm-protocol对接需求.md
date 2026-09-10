# llm-protocol 对接需求（Lumen 视角）

> 目的：把 Lumen 作为下游网关对 `llm-protocol`（F:\llm_protocol）的能力/接口需求整理成 back-log。
> 审计基线：`llm-protocol` @ `5cc4f57`，日期 2026-09-11。
> 说明：本文件只描述**缺口与建议接口**，不要求改动 Lumen；SDK 的职责边界（不做鉴权 / HTTP / 路由）保持不变。

## 0. 背景：Lumen 对 SDK 的期望位置

Lumen 是本机 LLM 网关：按别名路由、转发上游、记录用量与花费。当前只做同协议透传；接入本 SDK 后，期望在 `gateway/` 内使用它完成**跨协议转换**（OpenAI Chat ↔ Anthropic Messages），并让**用量保真**成为可以直接信任的一层。

Lumen 已有的相关逻辑（期望被 SDK 覆盖或对齐）：

- `src-tauri/src/gateway/usage.rs`：六类规范量 + `UsageSource`（`provider` / `estimated` / `partial` / `missing`）+ 缓存边界计费（`contains_cache_read`）。
- `src-tauri/src/gateway/forward.rs`：SSE 逐块扫描、`stream_options.include_usage` 注入、流未收尾标记。

## 1. 结论摘要

SDK 的**转换语义**（thinking / tool call / 流式一致性 / 边界）已经足够成熟；短板集中在两处：

1. **用量保真**：OpenAI 协议方向缓存用量完全丢失，`Usage` 缺边界与可信度标记；
2. **输出侧 framing**：只有 SSE 解码，没有编码，也没有字节级流式门面。

## 2. P0 — 用量与计费

### G1. OpenAI 协议不解析缓存用量

- 现象：`src/codec/openai_chat/native.rs:1141` `decode_openai_usage` 把 `cache_read_tokens` / `cache_write_tokens` 写死为 `None`；`OpenAiUsage` DTO（`native.rs:214` 附近）没有 `prompt_tokens_details`。流式同样写死：`src/codec/openai_chat/stream.rs:569-575`。
- 对照：Anthropic 侧完整：`src/codec/anthropic/native.rs:1155`、`src/codec/anthropic/stream.rs:152`。
- 影响：Lumen 走 OpenAI 协议上游（DeepSeek / 百炼 / 火山等）的缓存计费在转换路径上归零。
- 建议：
  - 解析 `usage.prompt_tokens_details.cached_tokens` 与 `cache_write_tokens`；
  - 兼容 DeepSeek `prompt_cache_hit_tokens` / `prompt_cache_miss_tokens`（后者不可当写入量）；
  - 编码方向（`encode_openai_usage`）同步回写缓存字段。
- 参考：本仓库 `docs/LLM聚合网关协议返回格式与用量字段对照.md` §2、§9。

### G2. `Usage` 缺「缓存边界」标记

- 现象：`src/model/response.rs:66` 的 `Usage` 六类量齐全，但没有「input 是否已包含 cache_read」。
- 影响：Lumen 计费必须先扣命中再计价，边界只能靠协议推断；SDK 才是掌握来源的一方。
- 建议：在 `Usage` 增加 `cache_read_in_input: Option<bool>`，或在设计文档中把 per-protocol 约定固化并由 codec 填好。

### G3. 缺「只取用量」的协议感知接口

- 现象：用量只在 `Response.usage` 及 `StreamEvent::UsageUpdate` 中，必须解完整个 IR 才能拿到。
- 影响：Lumen 的**同协议透传**路径也想用协议感知的用量，而不想跑完整 IR，也不想再维护一套启发式解析。
- 建议：暴露 `codec.decode_usage(&Value) -> Option<Usage>` 与流式 chunk 级对应接口。

### G4. `Usage` 缺可信度来源

- 现象：SDK 原则是「缺失不捏造」，但没有 `provider / estimated / missing / partial` 之类来源标签，也未标记流是否正常收尾。
- 影响：Lumen 已定义 `UsageSource`（`gateway/usage.rs`），两侧不对齐就得在网关再套一层。
- 建议：IR 层直接携带来源/可信度。

## 3. P0 — 流式输出侧

### G5. 没有 SSE 编码器

- 现象：`src/sse.rs` 只有 `SseDecoder`（bytes → `SseFrame`），无反向。
- 影响：`StreamBridge` 输出 `NativeStreamEvent`，Lumen 必须自己拼 `event:` / `data:` 与 `[DONE]`。
- 建议：提供 `SseEncoder` 或 `NativeStreamEvent -> bytes` 的对称能力。

### G6. 缺「字节进 / 字节出」的流式门面

- 现象：Lumen 需自行组合 `SseDecoder` + `StreamBridge`（`src/bridge.rs:87`）。
- 影响：容易漏掉 EOF、`[DONE]`、错误收尾等边界。
- 建议：提供 `StreamingConverter::{push_bytes, finish}`，把 framing 与转换缝在一起。

### G7. 流中错误无目标协议错误编码

- 现象：`push_upstream` / `finish` 以 `Err(Error)` 返回（如 `InvalidToolArguments`），不产出下游协议的错误事件。
- 影响：Lumen 需自行合成 Anthropic `error` 事件 / OpenAI 错误 chunk。
- 建议：提供 `encode_stream_error(&Error) -> Vec<NativeStreamEvent>`。

## 4. P1 — 配置与可观测

| ID | 缺口 | 证据 | 对 Lumen 的影响 |
|---|---|---|---|
| G8 | `Diagnostic` / `DiagnosticCode` / `Severity` 无 serde 派生 | `src/conversion.rs:131-179` | 无法直接落日志 / 发事件，需手抄映射 |
| G9 | `DialectId` 是空壳，dispatch 只按 `Protocol` | `src/protocol.rs:27` | 无法把「OpenAI 兼容但字段有差异」的方言差异收敛进 SDK |
| G10 | 无 model 覆写入口 | `convert_request`（`src/bridge.rs:30`）保留 body.model | Lumen 需在转换后手改 `body["model"]`，IR 与最终请求不一致 |
| G11 | `encode_openai_usage` 在 total 缺失时用 `input+output` 兜底 | `src/codec/openai_chat/native.rs:1359` | 与「缺失不捏造」原则小冲突，至少应标注为 derived |

## 5. 已满足、无需补

- 鉴权 / header / HTTP / 路由边界干净，正好是 Lumen 的职责。
- `Codec::capabilities()`（`src/codec/mod.rs:23`）暴露 `requires_max_output_tokens` 等，Lumen 可据此驱动 `ConversionPolicy`。
- 流式 IR 覆盖 usage / finish / tool / reasoning / ping。
- 错误模型结构化且 `#[non_exhaustive]`，便于映射。
- `sse` 默认特性 + `default-features = false` 支持。

## 6. 最小接口愿望单（按性价比）

1. `SseEncoder` / `NativeStreamEvent::to_sse_bytes`
2. OpenAI 侧补齐 `cache_read` / `cache_write` 解析（含 DeepSeek 字段）
3. `Usage` 增加 `cache_read_in_input` 与 `source`
4. `decode_usage`（协议感知，不跑全 IR）
5. `StreamingConverter`（bytes ↔ bytes）
6. Diagnostics 增加 serde 派生

## 7. 明确不改的边界（避免误伤）

- SDK 不接收 / 不返回 / 不存储 API key；header 值由网关植入（对齐 `docs/plan/v2/03-transport-boundary.md`）。
- SDK 不发送 HTTP、不管超时与连接；Lumen 侧负责 `reqwest` 传输。
- 不为 Anthropic 缺失 `max_tokens` 猜默认值；由 Lumen 的 policy 显式提供。
