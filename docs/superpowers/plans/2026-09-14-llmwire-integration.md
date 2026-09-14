# llmwire 协议转换接入 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 Lumen 网关内接入通用协议转换核 `llmwire`，让**入站协议 ≠ 上游端点协议**的请求自动转换（当前 Chat / Messages / Responses 三协议、6 个有向组合），同时**保持同协议路径字节透传不变**。

**Architecture:** 候选解析时按 provider 选端点——**同协议端点优先（透传）**，无则回退到**可转换**协议端点（openai / anthropic / responses）。`handlers::forward` 在候选循环里判断 `needs_conversion`；需要转换时用请求级 `Converter` 转请求体、转非流式响应体、或在流式 relay 中逐块转换。**用量记账仍扫上游字节**（target 协议），与转换解耦；`Report` 只作诊断。

**Tech Stack:** Rust（axum / reqwest / rusqlite）、`llmwire`（本地路径依赖）、Tauri 2。

**Spec:** `docs/llmwire对接文档.md`（集成契约与验收标准 §4）；`docs/后续功能计划备忘.md` §5.3。

**依赖:** `llmwire` 以**本地路径**接入（`src-tauri/Cargo.toml` → `../../llmwire`），开发期不固定 commit。**P0（响应 `model` / `id` 透传）已由 llmwire 侧修复**（`8340d27`，交付基线 `153d74a`），见 `docs/llmwire移交报告.md`；`created` / `created_at` 仍编码为 `0`，不属本次范围。

## Global Constraints

- 网关层（`gateway/`）不依赖 Tauri；数据结构只在 `db/models.rs` 定义一处。
- 本计划**不改 DB schema、不改定价、不改前端**。
- 同协议路径的转发行为与字节必须**零变化**（现有集成测试须保持绿）。
- `llmwire` 的公共 API 变动只允许影响 `gateway/convert.rs` 这一处适配点。
- 验证命令（每任务结束都跑）：`cargo test`、`cargo clippy -- -D warnings`（于 `src-tauri/`）。

## 设计决议（已拍板）

- **端点选择顺序是「每 provider 内」的同协议优先**：某目标所属 provider 若提供入站协议的端点，则用该端点透传；否则回退到它提供的一个可转换端点。**跨 target 的先后仍按 `priority`**，不因是否转换而重排（用户可用 priority 自行把透传目标排前）。
- **Gemini 不参与转换**：入站或上游任一方为 `gemini` 时只能同协议透传；`select_endpoint` 的回退候选**只在** openai / anthropic / responses 之间。
- **响应 `model` 由 llmwire 透传**（目标行为见对接文档 §6.1）。Lumen **不**在响应方向改写 `model`。
- **请求改写归 Lumen**：`model` → 上游模型名；Chat / Responses 流式补 `stream_options.include_usage=true`（在**转换后**的 body 上做，因为形状属于目标协议）。
- **记账口径对齐 IR-INV-USAGE-2**：`input` 含 `cached`、`cache_creation` 独立；流式与应答用量继续由 `gateway/usage.rs` 扫**上游字节**（target 协议）取得。
- **能力策略 MVP 用 `llmwire::resolve(src, dst, model)`**；`StaticHost` / `ModelProfile` 装配点预留，不在本计划强制实现。
- **同步/异步**：`Converter` 为同步库，在 tokio 任务内直接调用；chunk 小，MVP 不 `spawn_blocking`（大体积非流式响应后续再评估）。

## 非目标（明确不做）

- 不做 Gemini 的跨协议转换（保持透传）。
- 不支持有状态 Responses（`store=true` / `previous_response_id`）的转换——显式拒绝。
- 不改 `routes` / `route_targets` / `provider_endpoints` 的数据模型。
- 不引入按模型的 thinking / max_tokens 策略表（MVP 用协议默认，见预留点）。
- 不在响应方向改 `model`。

---

### Task 1: 依赖接入与 `convert` 模块骨架（不接线）

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/gateway/convert.rs`
- Modify: `src-tauri/src/gateway/mod.rs`

- [x] `Cargo.toml` 增加本地路径依赖：`llmwire = { path = "../../llmwire" }`；`cargo build` 确认可编译。
- [x] `convert.rs` 暴露协议映射：`protocol_id(&str) -> Option<ProtocolId>`（`openai`/`anthropic`/`responses` → 对应，`gemini`/其它 → `None`）、`is_convertible(&str) -> bool`、`needs_conversion(inbound, upstream) -> bool`（两端都可转换且不相等）。
- [x] `convert.rs` 定义 `Conversion`：`new(inbound, upstream, model)` 内部 `llmwire::converter(src, dst, llmwire::resolve(src, dst, model))`；方法 `request(&[u8]) -> Result<Vec<u8>, AppError>`、`response(&[u8]) -> Result<Vec<u8>, AppError>`、`feed(&[u8]) -> Vec<u8>`、`finish() -> (Vec<u8>, Termination)`、`take_report() -> Report`。`llmwire::Error` → `AppError::message`。
- [x] `mod.rs` 注册 `pub mod convert;`。
- [x] 单测：`protocol_id` 覆盖 4 协议；`needs_conversion` 真值矩阵；Chat↔Messages 各一条请求/响应 golden（字面量 body 断言）。
- Verify: `cargo test`、`cargo clippy -- -D warnings`。

### Task 2: `resolve` 回退 + 非流式转换接线（Chat↔Messages 端到端）

**Files:**
- Modify: `src-tauri/src/gateway/resolve.rs`
- Modify: `src-tauri/src/gateway/handlers.rs`

- [x] `select_endpoint` 改为：同协议端点优先；否则取该 provider 第一个**可转换**端点（`convert::is_convertible`，按端点 rowid）；无则 `None`。
- [x] `resolve.rs` 测试：同协议优先命中；仅异协议可转换端点时回退且 `upstream_protocol != inbound`；provider 仅有 `gemini` 端点而入站为 `openai` 时落选。
- [x] `handlers::forward` 候选循环：`convert::needs_conversion(required_protocol, &candidate.upstream_protocol)` 为真时构造 `Conversion`；请求体先经 `conversion.request(...)`，再对**转换后**的 body 做 `model` 覆写与 `ensure_include_usage`；同协议分支保持现状。
- [x] 非流式 `AttemptAction::Served`：上游 2xx body 经 `conversion.response(...)` 后再 `passthrough`；用量继续从**上游 body** 以 `candidate.upstream_protocol` 提取（不改）。
- [x] 转换失败（`request`/`response` 返回 `Err`）落 error 日志并转下一候选；**不冷却上游**——失败源于本地转换而非上游。
- [x] handlers mock 测试：`chat` 请求路由到 anthropic-only provider → 断言上游收到 Anthropic 形状请求；响应转回 Chat 形状。
- Verify: `cargo test`、`cargo clippy -- -D warnings`。

### Task 3: 流式转换接线

**Files:**
- Modify: `src-tauri/src/gateway/forward.rs`
- Modify: `src-tauri/src/gateway/handlers.rs`

- [x] `stream_response` 增参 `conversion: Option<Conversion>`；spawn 任务内：无转换时原样 `tx.send(bytes)`；有转换时 `scanner.push(&bytes)`（用量仍扫上游）后 `let out = conversion.feed(&bytes)`，非空才发送。
- [x] 流结束：有转换则 `let (tail, term) = conversion.finish()` 发送 `tail`；`Termination` 非 `Explicit`/`CleanClose` 时记 `failure`（日志 `status=error`）。
- [x] handlers 流式分支把 `conversion` 移交 `stream_response`。
- [x] 测试：mock 上游 Anthropic SSE → 下游 Chat SSE（含 `[DONE]` 收尾）；同协议流式**字节不变**（回归断言）。
- Verify: `cargo test`、`cargo clippy -- -D warnings`。

### Task 4: 扩展组合与能力策略

**Files:**
- Modify: `src-tauri/src/gateway/convert.rs`
- Modify: `src-tauri/src/gateway/handlers.rs`

- [x] 覆盖 Responses 参与的 6 个有向组合（请求 + 响应 + 流式）的等价性测试。
- [x] 复核 caps：确认 `resolve(src, dst, upstream_model)` 的策略（thinking：目标 Chat 时 Strip，目标 Messages/Responses Passthrough；`passthrough_cache_control` 仅目标 Messages）符合预期；在 `convert.rs` 注释中标注后续 `StaticHost`/`ModelProfile` 装配点。
- Verify: `cargo test`、`cargo clippy -- -D warnings`。

### Task 5: 限制与可观测

**Files:**
- Modify: `src-tauri/src/gateway/convert.rs`
- Modify: `src-tauri/src/gateway/handlers.rs`

- [ ] 转换前置校验并**显式拒绝**（清晰错误，不静默）：Responses `store=true` / 带 `previous_response_id`；Chat 流式 `n > 1`；图片 `file_id`。仅在 `needs_conversion` 时生效。
- [ ] `Report` → `tracing` 日志：遍历 `unmapped` / `warnings`，映射 `Severity`；出现 `Fatal` 视为该次失败。
- [ ] 测试：上述被拒场景返回预期错误；`Report` 非空时日志可见（可用断言辅助函数）。
- Verify: `cargo test`、`cargo clippy -- -D warnings`。

### Task 6: 记账口径与同协议回归

**Files:**
- Modify（如需要）: `src-tauri/src/gateway/usage.rs`
- Modify（如需要）: `src-tauri/src/gateway/forward.rs`

- [ ] 核对 `usage.rs` 的 cache 边界判定与 `finalize` 口径，确认与 IR-INV-USAGE-2 一致（`input` 含 `cached`、`cache_creation` 独立），必要时补测试与注释；边界按 `upstream_protocol` 判定（现状已如此，确认无需改）。
- [ ] 跑全量现有集成测试，确认同协议路径**字节级不变**、账目数字不变。
- Verify: `cargo test`、`cargo clippy -- -D warnings`。

### Task 7: 验收

**Files:** —

- [ ] 逐项跑 `docs/llmwire对接文档.md` §4 验收清单，并对齐 `docs/llmwire移交报告.md` §13 清单，逐项勾选。
- [ ] 复核 P0 已落地：转换后响应 `model` / `id` 透传上游上报值；`created` / `created_at` 为 `0` 属已知限制。
- Verify: `cargo test`、`cargo clippy -- -D warnings`。

## 风险与注意

- **本地 path 依赖耦合**：llmwire 公共 API 变动会直接编译失败；`gateway/convert.rs` 是唯一适配点，改动应集中在此。
- **`created` / `created_at` 不透传**：llmwire 当前编码为 `0`，不得视为上游真实时间；`id` / `model` 已透传。
- **本地路径 dev-only**：正式复现/移交需固定 commit（见移交报告 §3.2）。
- **序列化 key 顺序**：转换后的请求由 llmwire 结构体决定 key 顺序，仅影响转换路径（同协议仍字节透传），对 prefix-cache 的影响限于转换请求本身。
- **首次落地范围**：先打通 Chat↔Messages 非流式→流式，再扩 Responses。
