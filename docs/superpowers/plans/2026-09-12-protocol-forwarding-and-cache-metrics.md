# Protocol Forwarding & Cache Metrics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在现有「同协议透传」网关上新增两种可转发协议——OpenAI **Responses**（端点 `/v1/responses`）与 Google **Gemini 原生**（端点 `/v1beta/models/{alias}:{action}`，流式强制上游 `alt=sse`）——并修正缓存命中率的分母口径（按协议边界计算，消除 OpenAI 系被重复计入命中的系统性低估）。

**Architecture:** 延续「路由锁定协议 + 目标协议同构 + 端点绑定协议」的既有约束，只做逐字节透传，不引入 IR 转换。协议差异收敛在四处：`upstream_path` 动态构造、鉴权头、响应用量提取/流式扫描、缓存边界标记。缓存边界在落库时固化，统计 SQL 按标记决定分母。

**Tech Stack:** Rust（axum 0.8 / reqwest 0.12 / rusqlite 0.32）、Tauri 2、React 19、TypeScript、Vite

**Spec:** `docs/LLM聚合网关协议返回格式与用量字段对照.md`（§2/§3/§6/§9）、`docs/后端接口文档.md`、`docs/UIUX设计文档`

**依赖:** 无。cache 口径修正与协议扩展可独立推进。

## 设计决议（已拍板）

- **范围仅限本协议透传**；不做任何跨协议转换（OpenAI ↔ Anthropic ↔ Responses ↔ Gemini）。
- 新增协议标识：`responses`、`gemini`，与 `openai` / `anthropic` 并列，provider 与 route 均锁定协议。
- **Gemini 走原生**：请求/响应形状不做改写；模型名在 URL path（body 无 `model`）。
- Gemini 流式端点**强制上游追加 `?alt=sse`**，换取标准 SSE 与网关复用现有扫描器；下游拿到 SSE 而非 Gemini 默认拼接 JSON。
- Gemini 鉴权用 `x-goog-api-key`（新增 AuthScheme），不把 key 放进 URL。
- Responses 端点 `/v1/responses`；沿用 `body.model`，流式靠 `response.*` 事件收尾。
- **缓存命中率**：把 `contains_cache_read` 边界落库（新列 `cache_read_in_input`），分母按边界计算：
  - `input` 已含命中（OpenAI / DeepSeek / Responses / Gemini）→ `分母 = input + cache_creation`
  - `input` 不含命中（Anthropic）→ `分母 = input + cache_read + cache_creation`
- 允许清库：`SCHEMA_VERSION` 6 → 7，启动按现有逻辑重建空库。

## Global Constraints

- 只透传，不改请求体/响应体语义；除 `model` 注入与 `alt=sse` 外不得改写上游数据。
- Gemini 不在 body 注入 `model`（模型只出现在 URL path）。
- 新增协议必须计入 `is_known_protocol` 与前端 `Protocol` 联合类型；路由/目标协议一致性校验沿用。
- 颜色与样式遵守 `AGENTS.md`：颜色只来自 `theme.css`，尺寸只来自 `layout.css`，页面选择器挂在根类下。
- 纯计算/格式化逻辑独立成不依赖框架的模块并配同目录测试。
- 接口契约变更后同步 `docs/后端接口文档.md`。
- 验证命令：`cargo test`、`cargo clippy -- -D warnings`（于 `src-tauri/`）、`pnpm test`、`pnpm build`。

## 非目标（明确不做）

- 不做 OpenAI ↔ Anthropic ↔ Responses ↔ Gemini 的跨协议转换。
- 不做 Gemini `countTokens` / `embedContent` / `files` / batch 等其它端点（仅 `generateContent` 与 `streamGenerateContent`）。
- 不解析 Gemini 内联多模态内容（音频/图片/视频）；原样透传。
- 不改 `reasoning_share`（`reasoning / output`）口径；仅修缓存命中率。
- 不做 Responses 的 `previous_response_id` / Conversations 状态管理（透传即可）。
- 不做 Responses / Gemini 的请求侧字段校验（缺 `model` 等由上游报错）。

---

### Task 1: 协议标识与后端校验

**Files:**
- Modify: `src-tauri/src/db/models.rs`

- [ ] 新增常量 `PROTOCOL_RESPONSES = "responses"`、`PROTOCOL_GEMINI = "gemini"`。
- [ ] `is_known_protocol` 纳入两者（`db/models.rs:14`）。
- [ ] 确认 `save_provider` / `save_route` / `seed` 的协议锁定与同构校验无需改动即可接受新值（未知协议仍拒绝）。
- [ ] 写测试：`is_known_protocol` 对四种协议返回 true、对未知返回 false；`save_route` 用 `gemini` 目标可保存、用未知协议报错。
- [ ] 运行 `cargo test` 至通过。

### Task 2: 用量提取支持 Responses / Gemini

**Files:**
- Modify: `src-tauri/src/gateway/usage.rs`
- Test: `src-tauri/src/gateway/usage.rs`（`#[cfg(test)]` 内联）

- [ ] `extract_fields` 扩展回退链（均在 `usage` 容器内取值）：
      - `input`：`prompt_tokens` → `input_tokens` → `promptTokenCount`
      - `output`：`completion_tokens` → `output_tokens` → `candidatesTokenCount`
      - `total`：`total_tokens` → `totalTokenCount`
      - `cache_read`：`cache_read_input_tokens` → `prompt_tokens_details.cached_tokens` → **`input_tokens_details.cached_tokens`** → `prompt_cache_hit_tokens` → `cachedContentTokenCount`
      - `cache_creation`：`cache_creation_input_tokens` → `prompt_tokens_details.cache_write_tokens`（Gemini/Responses 无独立写入量，留空）
      - `reasoning`：`completion_tokens_details.reasoning_tokens` → `output_tokens_details.reasoning_tokens` → `thoughtsTokenCount`
- [ ] `extract_usage` 改为按协议选择用量容器：`gemini` 取 `value["usageMetadata"]`，其余取 `value["usage"]`；`contains_cache_read` 由 `forward::contains_cache_read(protocol)` 决定（注意避免循环依赖：由调用方传入边界 bool，或把边界判断移到 `usage.rs`）。签名建议 `extract_usage(value: &Value, protocol: &str) -> Option<UsageTotals>`。
- [ ] 边界标记：`contains_cache_read` 对 `anthropic` 为 false，其余为 true（Responses 的 `input_tokens` 含缓存、Gemini 的 `promptTokenCount` 含缓存，见对照文档 §2/§6）。
- [ ] 写测试：
      - Responses 响应：`input_tokens` / `output_tokens` / `input_tokens_details.cached_tokens` / `output_tokens_details.reasoning_tokens` 正确解析；
      - Gemini 响应：`usageMetadata` 的 `promptTokenCount` / `candidatesTokenCount` / `cachedContentTokenCount` / `thoughtsTokenCount` / `totalTokenCount` 正确解析，且 `contains_cache_read=true`；
      - Gemini 无独立 cache_creation 时不臆造写入量。
- [ ] 运行 `cargo test` 至通过。

### Task 3: 转发路径、鉴权与模型注入

**Files:**
- Modify: `src-tauri/src/gateway/forward.rs`
- Modify: `src-tauri/src/gateway/handlers.rs`

- [ ] 用动态路径替换 `upstream_path_for`：`pub fn upstream_path(protocol: &str, model_id: &str, is_stream: bool) -> String`：
      - `anthropic` → `messages`
      - `responses` → `responses`
      - `gemini` → `models/{model_id}:generateContent`；流式 → `models/{model_id}:streamGenerateContent?alt=sse`
      - 其它 → `chat/completions`
- [ ] `send` 的鉴权分支增加 `x-goog-api-key`（`forward.rs:57`）：`"x-goog-api-key" => request.header("x-goog-api-key", &route.api_key)`，保持 bearer / x-api-key 行为不变。Gemini 不再有 `anthropic-version` 之类的固定头需要注入。
- [ ] `handlers::forward` 的模型注入改为协议感知：仅当协议使用 `body.model`（openai / anthropic / responses）时写 `body["model"] = route.model_id`；`gemini` 跳过（模型只在 path）。
- [ ] `contains_cache_read` 保留 `protocol != anthropic` 语义并加注释说明 Responses / Gemini 已含命中。
- [ ] 写测试：
      - `upstream_path` 四种协议 + Gemini 流式/非流式路径与 `alt=sse`；
      - `send` 对 `x-goog-api-key` 注入正确（用 mock 上游回显头）；
      - Gemini 转发后 body 不含被注入的 `model`。
- [ ] 运行 `cargo test` 至通过。

### Task 4: 流式扫描支持 Responses / Gemini

**Files:**
- Modify: `src-tauri/src/gateway/forward.rs`
- Test: `src-tauri/src/gateway/forward.rs`（`#[cfg(test)]` 内联）

- [ ] `UsageScanner` 携带 `protocol`（在 `stream_response` 中按 `route.upstream_protocol` 构造）。
- [ ] 收尾判定扩展：
      - Responses：`type ∈ {response.completed, response.incomplete, response.failed}`；
      - Gemini：chunk 含 `usageMetadata` 且 `candidates[].finishReason` 非空（Gemini SSE 无显式终止符，以「拿到最终用量」为收尾依据）；
      - 保留现有 `message_stop`（anthropic）与空 `choices` + usage（openai）。
- [ ] usage 定位回退链扩展：`value["usage"]` → `value["message"]["usage"]`（anthropic）→ **`value["response"]["usage"]`（responses completed）** → **`value["usageMetadata"]`（gemini）**。
- [ ] 保持流中断语义：未观察到收尾标记时 `UsageSource::Partial`，绝不当作完整。
- [ ] 写测试：
      - Responses 事件流（`response.output_text.delta` + `response.completed` 内嵌 usage）→ Provider，数值正确；
      - Responses 缺 `response.completed` → Partial；
      - Gemini SSE chunk（含 `usageMetadata` + `finishReason`）→ Provider；缺最终 chunk → Partial。
- [ ] 运行 `cargo test` 至通过。

### Task 5: 新端点与 handler

**Files:**
- Modify: `src-tauri/src/gateway/handlers.rs`
- Modify: `src-tauri/src/gateway/mod.rs`

- [ ] 重构 `forward` 使 `alias` / `is_stream` 可由调用方提供：新增内部函数 `forward_with(state, headers, body, required_protocol, endpoint, alias, is_stream)`；现有 `chat_completions` / `messages` 从 `body` 取值后调用它。
- [ ] 新增 `responses` handler：`POST /v1/responses`，`required_protocol = PROTOCOL_RESPONSES`，沿用 `body.model` + `body.stream`。
- [ ] 新增 `gemini_generate` handler：`POST /v1beta/models/{model_action}`，用 `axum::extract::Path` 取 `{model_action}`，按 `:` 拆成 `(alias, action)`：
      - `action = generateContent` → `is_stream = false`
      - `action = streamGenerateContent` → `is_stream = true`
      - 其它 action → 返回 `400` `lumen_error`（不转发、不落日志或落一条 error 日志，按现有 `reject` 模式）；
      - `required_protocol = PROTOCOL_GEMINI`；`endpoint` 记为 `/v1beta/models/{alias}:{action}`。
- [ ] `gateway/mod.rs` 注册：`.route("/v1/responses", post(handlers::responses))`、`.route("/v1beta/models/{model_action}", post(handlers::gemini_generate))`。
- [ ] 协议不匹配仍走 `ProtocolMismatch`（`handlers.rs:158`），返回 `400` 且不转发。
- [ ] 写测试：
      - Responses 非流式透传 mock 上游，落库 `endpoint = /v1/responses`、用量与花费正确；
      - Gemini `generateContent` 透传，mock 上游断言 URL path 为 `models/{real_model}:generateContent`、收到 `x-goog-api-key`、body 无注入 `model`；
      - Gemini `streamGenerateContent` 上游 URL 带 `alt=sse`；
      - Gemini 路由在 `/v1/chat/completions` 调用返回 400 `protocol_mismatch`；反向亦然。
- [ ] 运行 `cargo test` 至通过。

### Task 6: 缓存边界落库与 schema 升级

**Files:**
- Modify: `src-tauri/src/db/mod.rs`
- Modify: `src-tauri/src/db/models.rs`
- Modify: `src-tauri/src/db/logs.rs`
- Modify: `src-tauri/src/gateway/usage.rs`

- [ ] `SCHEMA` 的 `request_logs` 增加列 `cache_read_in_input INTEGER NOT NULL DEFAULT 0`；`SCHEMA_VERSION` 6 → 7（启动重建空库，已同意）。
- [ ] `RequestLog` 增加字段 `pub cache_read_in_input: bool`；`from_row` 用 `!= 0` 解析。
- [ ] `db/logs.rs`：`INSERT` 列清单与参数、`SELECT` 列表同步新列（`logs.rs:102` 附近）。
- [ ] `build_log`（`usage.rs`）写入 `cache_read_in_input: usage.contains_cache_read`。
- [ ] 同步 `keys.rs` / `stats.rs` / `logs.rs` 内所有 `RequestLog { .. }` 测试构造与任何 `SELECT *` 消费点，补默认值。
- [ ] 写测试：一条 `contains_cache_read=true` 与一条 `false` 的日志落库后读回，布尔与取值正确。
- [ ] 运行 `cargo test` 至通过。

### Task 7: 命中率分母按边界计算

**Files:**
- Modify: `src-tauri/src/db/stats.rs`
- Modify: `src-tauri/src/db/attribution.rs`

- [ ] 三处 SQL 的输入侧分母替换为按边界求和（`stats.rs:291`、`stats.rs:455`、`stats.rs:620`）：
      ```sql
      SUM(CASE WHEN cache_read_in_input = 1
               THEN input_tokens + cache_creation_tokens
               ELSE input_tokens + cache_read_tokens + cache_creation_tokens END)
      ```
      `cache_read` 分子保持 `SUM(cache_read_tokens)`。
- [ ] 更新 `attribution.rs:13` 的口径注释，并确认 `build_attribution` 只是消费 `(cache_read, input_side)`，无需改算法。
- [ ] 写测试：
      - `contains_cache_read=true`：input=100、cache_read=60 → 命中率 0.6（旧公式会得 0.375）；
      - `contains_cache_read=false`：input=40、cache_read=60 → 命中率 0.6；
      - 两类混合记录时按各自分母合并；
      - 归因的 `CacheShift` 使用修正后口径。
- [ ] 运行 `cargo test` 与 `cargo clippy -- -D warnings` 至通过。

### Task 8: 前端协议枚举、类型与 UI

**Files:**
- Modify: `src/services/protocol.ts`
- Modify: `src/services/config.ts`
- Modify: `src/services/gateway.ts`
- Modify: `src/features/providers/providerModel.ts`
- Modify: `src/features/providers/ProvidersPage.tsx`
- Modify: `src/features/routing/RoutingPage.tsx`
- Modify: `src/services/usage.ts`（mock）

- [ ] `Protocol` 联合类型增加 `"responses" | "gemini"`；`protocolLabel` 增加 `Responses` / `Gemini`。
- [ ] `AuthScheme` 联合类型增加 `"x-goog-api-key"`；`authSchemeLabel` 增加对应文案；`ProvidersPage` 鉴权下拉增加选项（`ProvidersPage.tsx:139`）。
- [ ] `ProvidersPage` 协议下拉增加两项（`ProvidersPage.tsx:147`）；`RoutingPage` 协议下拉增加两项（`RoutingPage.tsx:137`）。
- [ ] `gateway.ts` 的 `RequestLog` 增加 `cacheReadInInput: boolean`；`usage.ts` 的 mock 日志补齐该字段。
- [ ] 浏览器 mock：协议枚举扩展后 `pnpm dev` 不报类型错；必要时给 mock provider/route 增加一条 Gemini 或 Responses 示例。
- [ ] 运行 `pnpm test` 与 `pnpm build` 至通过。

### Task 9: 文档同步

**Files:**
- Modify: `docs/后端接口文档.md`

- [ ] `Protocol` 类型补 `responses` / `gemini`；`AuthScheme` 补 `x-goog-api-key`。
- [ ] HTTP 端点表补：
      - `POST /v1/responses`（OpenAI Responses 兼容；`model` 填别名）
      - `POST /v1beta/models/{alias}:generateContent` 与 `:streamGenerateContent`（Gemini 原生；模型在 path）
- [ ] 说明协议绑定：`responses` 只从 `/v1/responses` 调用、`gemini` 只从 `/v1beta/models/...` 调用；不匹配返回 `400 protocol_mismatch`。
- [ ] 说明 Gemini 流式网关向上游追加 `alt=sse`（下游得到 SSE），鉴权走 `x-goog-api-key`。
- [ ] `RequestLog` 增加 `cacheReadInInput`；补充缓存命中率口径（按边界计算的分母公式）。
- [ ] 说明 `SCHEMA_VERSION` 升级会重建空库、日志清空。
- [ ] 运行 `cargo test`、`cargo clippy -- -D warnings`、`pnpm test`、`pnpm build` 至通过。

### Task 10: 端到端验证

**Files:** 无新增

- [ ] 起网关，配置一个 `responses` 协议 provider + 模型 + 路由，用 `curl` 打 `/v1/responses`（含流式），确认透传、`endpoint` 落库、末块 usage 与花费正确。
- [ ] 配置 Gemini provider（`baseUrl = https://generativelanguage.googleapis.com/v1beta`，`authScheme = x-goog-api-key`）+ 路由，用 `curl` 打 `/v1beta/models/{alias}:generateContent` 与 `:streamGenerateContent`，确认 path/鉴权/`alt=sse` 与用量。
- [ ] 断言 Gemini body 未被注入 `model`；流式用量来源为 `provider`（拿到 `usageMetadata`）或 `partial`（中断）。
- [ ] 构造 OpenAI（含缓存命中）与 Anthropic（含缓存）两类日志，核对用量页「缓存命中」百分比符合修正后口径（OpenAI 不再被低估）。
- [ ] 协议错配（Gemini 路由调 `/v1/chat/completions`）返回 400 且不产生上游请求。
