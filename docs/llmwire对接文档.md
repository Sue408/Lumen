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

---

## 3. Lumen 明确不做（避免把复杂度放错位置）

- 不在 Lumen 重实现协议映射（交给 `llmwire`）。
- 不让 `llmwire` 知道别名、路由、模型、价格、预算。
- 不把 `Report` 当记账真源。
- 不向 IR 塞 `UsageSource` / 预算 / 日志格式等账单语义。

---

## 4. 验收标准（接入完成 = 这些全过）

- [ ] Chat / Messages / Responses 的 6 个有向组合，非流式文本转换正确。
- [ ] 6 个有向组合的流式转换，`Termination` 语义正确。
- [ ] tool call / tool result 跨协议往返，`tool_use.id` 字节保真。
- [ ] thinking / signature / encrypted reasoning 按策略透传或显式降级（不伪造）。
- [ ] OpenAI / Anthropic 缓存用量在转换后仍可被 Lumen 记账，且 cache 边界与 IR 约定一致。
- [ ] 同协议路径字节级未变（回归）。
- [ ] model 改写 + `include_usage` 注入后，上游请求正确。
- [ ] 错误映射：上游 4xx / 5xx、流中错误、`Report` 的 `Fatal` 条目。

---

## 5. 边界不变

- SDK 不接收 / 不返回 / 不存储 API key；header 由 Lumen 植入。
- SDK 不发 HTTP、不管超时与连接；传输由 Lumen 的 `reqwest` 负责。
- 核心无跨请求状态（请求级 `Converter`，用完即弃）。

---

## 附录：通用能力愿望单（非接入阻塞项）

> 以下都是「任何 host 都可能受益」的**通用**能力，不是 Lumen 的特有需求；接入不依赖它们。

| 能力 | 说明 |
|---|---|
| Gemini 原生支持 | 扩大协议覆盖到 4 协议 |
| OpenAI 兼容方言（dialect） | 收敛私有用量字段（如 DeepSeek `prompt_cache_hit_tokens` / `prompt_cache_miss_tokens`） |
| `Report` 家族 serde 派生 | 让 `Unmapped` / `Warning` / `Severity` 可直接序列化 |
| 通用观察钩子 | 让 host 免二次解析即可取已消费 / 产出的 IR（不专为计费） |
