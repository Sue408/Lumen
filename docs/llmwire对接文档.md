# llmwire 对接文档（Lumen 视角）

> 目的：定义 Lumen 作为本机网关，如何接入通用协议转换核 `llmwire`（`F:\llmwire`）。
> 基线：`llmwire` 0.1.0（`F:\llmwire @ cb4ef0b`，2026-09-14）。
> 性质：本文是 **Lumen 的集成契约**，只写「Lumen 怎么用通用 API」「Lumen 侧要写什么适配」「接入验收标准」。
> 立场：`llmwire` 是**无状态、无身份、无路由**的协议语义等价器。Lumen 特有的计费 / 路由 / 观测语义**不进入 SDK**，因此本文不再以「缺口 / 建议接口」组织，而以「Lumen 侧适配」组织。

**边界准则**：`llmwire` 回答「这段字节换成另一种协议，语义还等价吗」；Lumen 回答「能不能用、算谁的钱、发去哪、留什么痕」。

---

## 1. SDK 提供什么（Lumen 只用这些）

### 1.1 请求级转换器

`converter(src, dst, caps) -> Result<Box<dyn Converter>, Error>`，其中 `src` = 客户端在用的协议，`dst` = 上游端点协议。

```rust
trait Converter {
    fn request(&mut self, body: &[u8], out: &mut Vec<u8>) -> Result<(), Error>;
    fn response(&mut self, body: &[u8], out: &mut Vec<u8>) -> Result<(), Error>;
    fn feed(&mut self, chunk: &[u8], out: &mut Vec<u8>) -> Result<(), Error>;
    fn finish(&mut self, out: &mut Vec<u8>) -> Result<Termination, Error>;
    fn take_report(&mut self) -> Report;
}
```

- 时序：非流式 `request → response → take_report`；流式 `request → feed* → finish → take_report`。
- `out` 为**追加**语义，SDK 不清空，调用方复用缓冲区前自行 `clear()`。
- 必须先 `request` 才能 `feed` / `finish`；对流式请求调 `response` 返回 `Error::Protocol`。
- `Mode::NativePassthrough` 只是能力策略标签，**不构成字节旁路**：`Converter` 始终走「解码 → IR → 重编码」。

### 1.2 能力与策略

- 简单场景：`resolve(inbound, backend, model) -> Capabilities`。
- 按模型覆盖：`StaticHost` / `ModelProfile` / `Host` / `UnknownModelPolicy`。
- `Capabilities { mode, thinking, tool_id, passthrough_cache_control, passthrough_betas, supported: ParamSet }`。

### 1.3 低层 codec（自取用量用）

- `ProtocolCodec`（`Chat` / `Messages` / `Responses`）：`decode_response`、`decode_stream_frame`。
- `SseFramer`、`StreamState::usage()`：不跑 `Converter` 也能取规范用量。

### 1.4 诚实降级

- `Report { unmapped, warnings }`、`Severity { Silent, Degraded, Fatal }`、`UnmappedReason`。
- 当前 `Report` 家族**无 serde 派生**，Lumen 侧自行映射后落日志 / 事件。

### 1.5 协议覆盖

| 能力 | 状态 |
|---|---|
| OpenAI Chat Completions | 请求 / 响应 / 流式 |
| Anthropic Messages | 请求 / 响应 / 流式 |
| OpenAI Responses | 无状态请求 / 响应 / 流式 |
| 三协议双向（6 个有向组合） | 支持 |
| tool call / result、thinking、图片输入 | 支持或显式降级 |
| Google Gemini | **不在内**，Lumen 保持透传 |

---

## 2. Lumen 侧接入架构

### 2.1 方向与旁路规则

- `inbound_protocol` 来自 handler（`chat` / `messages` / `responses`）；`endpoint.protocol` 来自 `gateway/resolve.rs` 选中的端点。
- **相同**：字节透传，不构造 `Converter`（保上游私有字段与 prefix-cache 字节）。
- **不同**：构造 `converter(inbound, endpoint, caps)`，跨协议转换。

```text
request → resolve_candidates(alias, inbound_protocol)
        → 每个候选 endpoint.protocol
            == inbound ? 原样转发
                       : converter(inbound, endpoint, caps)
```

### 2.2 能力装配

- Lumen 模型表 → `ModelProfile`（`thinking` / `max_output_tokens` / `supported` / `passthrough_cache_control`）。
- 经 `StaticHost`（或自定义 `Host`）产出 `Capabilities` 交给 `converter`。
- `llmwire` 只提供**机制**（caps 结构），**取值策略**属 Lumen。

### 2.3 请求改写（Lumen 职责）

`Converter::request` 只做协议等价，不做 Lumen 的意图：

- **model**：会把入站 `model`（别名）原样写进上游请求。Lumen 必须在转发前把它改成上游模型名。
- **stream_options**：不会注入。Chat / Responses 流式需 Lumen 补 `stream_options.include_usage=true`，否则上游不回 usage。
- **max_tokens**：Chat → Messages 缺省时 `llmwire` 补 4096 并上报；Lumen 若要求按模型上限，自行改写。

建议统一在 `gateway/forward.rs` 转发前对已转换 body 做一次 JSON patch（改 model + 注 stream_options），避免在多处散落。

> **响应 model** 不归 Lumen：由 llmwire 透传 target 上报值（见 §6.1）；与同协议透传路径一致，客户端看到的是上游模型名。

### 2.4 流式接缝与终止

- `llmwire` 是同步库，Lumen 决定 chunk 转发与 flush 策略、是否 `spawn_blocking`。
- `Termination`（`Explicit` / `CleanClose` / `ClientAbort` / `Timeout` / `NetworkError`）→ 客户端关闭语义与日志由 Lumen 决定。
- 首字节已发出后不可降级；失败降级链策略在 `gateway/failover.rs`，`llmwire` 只报错。

### 2.5 记账（Lumen 职责）

- 采用 IR 约定：`Usage.input` **已含** `Usage.cached`；`cache_creation` 独立不计入 `input`（`llmwire` IR-INV-USAGE-2）。
- `UsageSource` 由 `Option`（`None` = 未知）、`Termination`、`Report` 推导，映射在 Lumen 侧。
- 流式记账**继续用 Lumen 自家** `gateway/usage.rs` 扫**上游字节**（target 协议），不依赖 `Converter` 暴露用量——记账与转换解耦。
- 定价、缓存读 / 写单价、预算全在 Lumen（`gateway/usage.rs` / `gateway/quota.rs`）。

### 2.6 观测与留存

- `Report` → `gateway` 日志 / 事件（Lumen 手抄映射）。
- 会话捕获、请求日志、DB 写入在 `gateway/session.rs` 与 `db/`。

### 2.7 落地清单（Lumen 侧模块）

1. **`gateway/convert.rs`（新增）**：方向分流（旁路 / 转换）、`Capabilities` 装配、请求改写（`model` / `stream_options`）、`Report` 收集、`Termination` 映射。响应 `model` 由 llmwire 透传（见 §6.1），Lumen 不改写。
2. **`gateway/forward.rs` / `gateway/handlers.rs`**：接入转换分支；非流式走 `response`，流式走 `feed` / `finish`；同协议路径保持现有透传不动。
3. **`gateway/resolve.rs`**：候选解析放开「同协议」限制——优先同协议端点（透传），无则回退到可转换端点（Chat / Messages / Responses）并标记需转换。
4. **`gateway/failover.rs`**：首字节前按候选降级；首字节后不再降级。
5. **`gateway/usage.rs`**：继续扫**上游字节**取用量；口径对齐 IR-INV-USAGE-2。
6. **`db/` / `gateway/session.rs`**：请求日志与 `Report` 落库 / 落事件。
7. **限制处理**：stateful Responses、Chat 流式 `n > 1`、Gemini、图片 `file_id` → 显式拒绝或降级，不静默。
8. **测试**：见 §4；同协议路径必须有字节级回归。

---

## 3. Lumen 明确不做（避免把复杂度放错位置）

- 不在 Lumen 重实现协议映射（交给 `llmwire`）。
- 不让 `llmwire` 知道别名、路由、模型、价格、预算。
- 不把 `Report` 当记账真源。
- 不向 IR 塞 `UsageSource` / 预算 / 日志格式等账单语义。

---

## 4. 验收标准（接入完成 = 这些全过）

> 状态更新：2026-09-14，Lumen 侧接线已完成（commit `9225cce` 及之前），下列各项均已通过；括号内为证据。

- [x] Chat / Messages / Responses 的 6 个有向组合，非流式文本转换正确。（`gateway/convert.rs::converts_all_six_directed_pairs`）
- [x] 6 个有向组合的流式转换，`Termination` 语义正确。（`streams_all_six_directed_pairs`；`Explicit`/`CleanClose` 视为成功，其余记为失败）
- [x] tool call / tool result 跨协议往返，`tool_use.id` 字节保真。（llmwire `tests/golden_tool_id.rs`；Lumen 不改写工具字段）
- [x] thinking / signature / encrypted reasoning 按策略透传或显式降级（不伪造）。（`llmwire::resolve` 策略：目标 Chat 剥离、目标 Messages/Responses 透传；`Report` 记录降级）
- [x] OpenAI / Anthropic 缓存用量在转换后仍可被 Lumen 记账，且 cache 边界与 IR 约定一致。（`handlers.rs::conversion_accounts_usage_with_upstream_cache_boundary`；边界按**上游协议**判定）
- [x] 同协议路径字节级未变（回归）。（现有集成测试全绿；同协议不构造 `Conversion`）
- [x] model 改写 + `include_usage` 注入后，上游请求正确。（`build_attempt`：先转换再按**上游协议**改写 `model` 与注入 `include_usage`）
- [x] 错误映射：上游 4xx / 5xx、流中错误、`Report` 的 `Fatal` 条目。（沿用既有失败分类；`Conversion::log_report` 把 `Fatal` 升级为该次失败）

> 已知限制（非验收项）：转换后的 `created` / `created_at` 目前为 `0`，不保证透传（见 `docs/llmwire移交报告.md` §6.4）。

---

## 5. 边界不变

- SDK 不接收 / 不返回 / 不存储 API key；header 由 Lumen 植入。
- SDK 不发 HTTP、不管超时与连接；传输由 Lumen 的 `reqwest` 负责。
- 核心无跨请求状态（请求级 `Converter`，用完即弃）。

---

## 6. SDK 侧待办（建议先提给 llmwire）

> 只列**通用**问题：任何 host 都会遇到，不是 Lumen 的特有需求。
>
> **状态（2026-09-14）**：P0 已由 llmwire `8340d27` 修复（交付基线 `153d74a`），响应 `model` 与 `id` 均透传上游上报值；下表保留作为问题记录与验证依据。`created` / `created_at` 仍为 `0`，见 `docs/llmwire移交报告.md` §6.4。

### 6.1 P0：输出的响应 `model` 未透传（已定：应等于 target 上报的 model）

**期望行为**：转换后输出中的 `model` 应等于 **target（上游端点）协议响应里上报的 `model`**，即透明透传；llmwire 不发明模型名，也不回填入站 `model`。

**根因**：响应 IR（`AssistantOutput`）没有 `model` 字段，非流式从解码到编码无处携带；且三个 `*ResponseOut` 的 `model` 字段类型是 `&'static str`，结构上只允许字面量常量。

**非流式：解码丢弃 → 编码写死 `"llmwire"`**

| 协议 | 解码 In（无 `model`） | 编码 Out | 写死点 |
|---|---|---|---|
| Chat | `ChatResponseIn`（`codec/chat/wire.rs:97`，无字段） | `ChatResponseOut.model: &'static str`（`chat/wire.rs:256`） | `chat/mod.rs:194` |
| Messages | `MessagesResponseIn`（`messages/wire.rs:256`，无字段） | `MessagesResponseOut.model: &'static str`（`messages/wire.rs:279`） | `messages/mod.rs:171` |
| Responses | `ResponsesResponseIn`（`responses/wire.rs:41`，无字段） | `ResponsesResponseOut.model: &'static str`（`responses/wire.rs:188`） | `responses/mod.rs:194` |

**流式：已基本正确**，`Event::MessageStart { model }` 已携带 target model：

- 解码填充：`chat/stream.rs:36`、`messages/stream.rs:29`、`responses/stream.rs:45 / 54`。
- 编码输出：`chat/stream.rs:171`（首个 chunk）、Messages 的 `message_start`、`responses/stream.rs:217` 与 `:773-775`（终帧取自 `state.message()`）。
- **残留**：Chat 作为 source 时，首个 chunk 之后的 chunk 仍写死 `"llmwire"`（`chat/stream.rs:190 / 215 / 230 / 255 / 277`）。
- **fallback 不一致**：target 未上报 model 时，chat 用 `""`（`unwrap_or_default`）、responses 用 `"llmwire"`。

**不是 bug 的地方（勿改）**：请求侧 `MessagesRequestOut.model`（`messages/mod.rs:101`）与 `ResponsesRequestOut.model`（`responses/mod.rs:110`）虽为 `"llmwire"`，但 `converter.rs:684` 的 `set_request_metadata` 会用入站 body 的 `model` 覆盖；请求侧由 host 负责。

**建议改动清单**

1. `AssistantOutput` 增加 `model`（如 `pub model: Option<Box<str>>`）；`chat` / `messages` / `responses` 的 `decode_response` 填充。
2. 三个 `*ResponseOut.model` 由 `&'static str` 改为 `String`；`encode_response` 写入该值。
3. Chat 流式后续 chunk 复用已捕获的 model，不再写死。
4. 定义 target 未上报时的 fallback（建议省略或空串，勿发明名字），并统一三协议。
5. 测试：三协议 ×（非流式 / 流式），断言 **target model ≠ 入站 model** 时输出 `model` 恒等于 target 上报值。

**同类观察（可选一并处理）**：非流式响应的 `id` 同样是「解码丢弃 → 编码编造」（`chatcmpl-llmwire` / `msg_llmwire` / `resp_llmwire`），而流式 `id` 已透传。若要真正透明，`id` 与 `model` 同源同修。

### 6.2 P1：通用增强（非接入阻塞项）

> 都是「任何 host 都可能受益」的能力，接入不依赖它们。

| 能力 | 说明 |
|---|---|
| Gemini 原生支持 | 扩大协议覆盖到 4 协议 |
| OpenAI 兼容方言（dialect） | 收敛私有用量字段（如 DeepSeek `prompt_cache_hit_tokens` / `prompt_cache_miss_tokens`） |
| `Report` 家族 serde 派生 | 让 `Unmapped` / `Warning` / `Severity` 可直接序列化 |
| 通用观察钩子 | 让 host 免二次解析即可取已消费 / 产出的 IR（不专为计费） |
