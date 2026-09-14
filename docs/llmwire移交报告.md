# llmwire 移交报告（Lumen 接入基线）

> 报告日期：2026-09-14
> 交付对象：Lumen 开发
> 交付基线：`llmwire` `main` @ `153d74a4da7df82a3c7a3b848d671429508fc8a2`
> 仓库地址：`https://github.com/apneasu/llmwire`（public，MIT）
> P0 提交：`8340d27 fix(codec): preserve response metadata`
> 交付提交：`153d74a fix(ci): track cargo lockfile`
> 配套文档：同目录 `llmwire对接文档.md`、`F:\llmwire\README.md`、`F:\llmwire\docs\spec\*`

---

## 1. 交付结论

`llmwire` 当前可以作为 Lumen M3 跨协议转换的集成基线。

本次交付已经修复 Lumen 对接文档中列出的 P0 问题：响应 `model` 未透传。并且把同类响应 `id` 一并修正为非流式与流式一致透传。

边界保持不变：

- `llmwire` 是同步、无状态、无传输依赖的协议转换核。
- HTTP、鉴权、路由、模型名改写、用量记账、日志、预算、重试与降级仍由 Lumen 负责。
- 同协议请求继续由 Lumen 字节透传，不经过 `Converter`。
- 跨协议请求才构造 `converter(src, dst, caps)`。

公开仓库已经建立，`main` 已推送且 CI 全绿。依赖应固定到交付 commit，不要浮动跟随 `main`。

---

## 2. 本次 P0 修正内容

### 2.1 IR 增加响应元数据

`AssistantOutput` 新增：

```rust
pub id: Option<Box<str>>,
pub model: Option<Box<str>>,
```

语义：

- `Some`：来自 target 上游响应实际上报值。
- `None`：target 未上报该字段。
- 编码到源协议时，`model` 缺失统一写空字符串。
- 不回填客户端请求里的模型名。
- 不发明 `"llmwire"` 等占位模型名。

### 2.2 三协议均支持响应 id/model 透传

非流式 Chat / Messages / Responses 均从上游响应解码 `id` / `model`，并在编码回客户端协议时写回。

### 2.3 Chat 流式后续 chunk 不再写死占位值

Chat 的首个 chunk 触发 `MessageStart`，后续 text、tool call、usage、finish chunk 全部复用首包捕获的 `id` / `model`。因此客户端看到的不是首包正确、后续又变回 `"llmwire"`。

### 2.4 fallback 统一

target 未上报 `model` 时，三协议统一输出：

```json
"model": ""
```

理由：

- 保留协议字段结构，避免客户端因字段缺失直接解析失败。
- 明确表达“上游未上报”，不冒充任何模型名。
- 与“不得发明 target model”的透明原则一致。

---

## 3. 依赖与构建约束

### 3.1 Rust 要求

- `edition 2021`
- 同步核心，不需要 async runtime
- 核心依赖仅允许：
  - `serde`
  - `serde_json`
  - `thiserror`
  - `bitflags`

禁止把以下依赖带进核心：

- `tokio`
- `reqwest`
- `async-trait`
- `anyhow`

### 3.2 Lumen 依赖建议

若 Lumen 与 `llmwire` 不在同一仓库，应使用 Git 固定 revision：

```toml
[dependencies]
llmwire = { git = "https://github.com/apneasu/llmwire.git", rev = "153d74a4da7df82a3c7a3b848d671429508fc8a2" }
```

若同机/同仓库开发，可使用本地路径：

```toml
[dependencies]
llmwire = { path = "../llmwire" }
```

本地路径方案只适合开发期；正式移交和复现应固定 commit。

---

## 4. 基本使用方式

### 4.1 方向约定

```rust
converter(src, dst, caps)
```

- `src`：客户端正在使用的协议。
- `dst`：Lumen 选中的上游端点协议。

```text
Chat      -> Messages
Chat      -> Responses
Messages  -> Chat
Messages  -> Responses
Responses -> Chat
Responses -> Messages
```

同协议不构造 `Converter`，继续字节透传。

### 4.2 生命周期

非流式：

```text
request -> response -> take_report
```

流式：

```text
request -> feed* -> finish -> take_report
```

调用约束：

- 必须先调用 `request`，之后才能 `feed` / `finish`。
- 对流式请求调用 `response` 会返回 `Error::Protocol`。
- `out` 是追加语义，SDK 不清空。
- 调用方复用 buffer 前自行 `clear()`。

### 4.3 同步与阻塞

`llmwire` 内部完全同步。

Lumen 若在 `async` handler 中执行大流量转换，应评估：

- 直接同步调用是否足够快。
- 是否用 `spawn_blocking` 隔离 CPU 密集型转换。
- 流式路径是否按 chunk 转换后立即 flush。

Lumen 自己决定调度策略，`llmwire` 不感知异步 runtime。

---

## 5. 请求侧注意事项

### 5.1 model 改写属于 Lumen

`Converter::request` 会尽量保留入站请求中的 `model`，但不会知道 Lumen 的别名到上游模型映射。

Lumen 必须在转发前把转换后的 JSON body 中 `model` 改为真实上游模型名。

建议统一在 `gateway/forward.rs` 或独立 `gateway/convert.rs` 做一次 JSON patch，不要在多个 handler 分散处理。

### 5.2 stream_options.include_usage 属于 Lumen

`llmwire` 不自动注入 `stream_options.include_usage=true`。

Lumen 需要按上游协议和端点能力补：

```json
{
  "stream": true,
  "stream_options": {
    "include_usage": true
  }
}
```

是否注入、对哪些端点注入，由 Lumen 能力表决定。

### 5.3 请求侧占位 model 不是 bug

Messages / Responses request wire struct 中出现 `"llmwire"` 占位不属于本次问题。

原因是 `Converter` 在请求编码后使用 `set_request_metadata` 覆盖入站 `model` 和 `stream`。Lumen 不应依赖低层 codec 直接编码的请求体，除非自行补齐 model。

### 5.4 max_tokens 默认值

Chat -> Messages 缺省 `max_tokens` 时，`llmwire` 会补 `4096` 并记 `Report`。

Lumen 若需要按模型表覆盖上游最大输出 token，应在转换后自行 patch。

---

## 6. 响应侧注意事项

### 6.1 响应 model 不由 Lumen 重写

P0 修复后，`llmwire` 输出响应中的 `model` 等于 target 上游响应上报的 `model`。

Lumen 不应：

- 回填客户端请求模型名。
- 回填路由别名。
- 把 `""` 改成某个猜测模型名。

如果上游没有上报模型，客户端看到 `"model": ""` 是预期行为。

### 6.2 Chat 流式所有 chunk 都使用目标元数据

Lumen 不应只检查首个 chunk。

P0 后，Chat 流式后续 text / tool / usage / finish chunk 都会复用目标 `id` / `model`。

### 6.3 id 透传

非流式响应 `id` 与 Chat 流式 `id` 都会优先透传 target 上报值；缺失时写空字符串。

注意：ID 可能带有上游协议风格，例如 `chatcmpl-*`、`msg_*`、`resp_*`。跨协议转换时 `llmwire` 不重写前缀，与流式既有行为保持一致。

### 6.4 created / created_at 目前不保证透传

本次 P0 只处理 `id` / `model`。

当前：

- Chat `created` 仍编码为 `0`。
- Responses `created_at` 仍编码为 `0`。

Lumen 不得把这些字段视为上游真实时间。若未来要求透明透传，应另行提出 P0/P1 变更。

---

## 7. 流式与终止语义

### 7.1 feed / finish

- `feed` 接受任意上游字节切片。
- 内部完成 SSE 分帧与跨 chunk 拼接。
- `finish` 冲刷尚未完成的帧并返回终止原因。

### 7.2 Termination

```rust
Termination::Explicit
Termination::CleanClose
Termination::ClientAbort
Termination::Timeout
Termination::NetworkError
```

Lumen 负责把这些值映射成客户端关闭语义、日志和请求状态。

注意：

- 无终止符的普通关闭不等于成功。
- 首字节发出后不可降级到其他上游。
- 降级链只适用于建连与首块前的失败。

### 7.3 流内错误

流内错误走信道 B：

- 已发出 HTTP 200 后，不返回普通 `Result::Err` 作为客户端错误。
- SDK 生成目标协议错误事件，并写入 `Report`。
- Lumen 应转发已经编码的错误事件并记录报告，不应把该请求当作成功完成。

---

## 8. Usage 与记账

Lumen 当前策略可以继续保持：流式记账扫描上游 target 协议字节，不依赖 `Converter` 暴露用量。

IR usage 约定：

- `Usage.input` 已包含 `Usage.cached`。
- `cache_creation` 独立，不计入 `Usage.input`。
- `None` 表示未知，`0` 表示确实是零。
- 不得把未知 usage 写成 0。

`Report` 是质量与降级报告，不是记账真源。

---

## 9. Report 与错误处理

`Report` 家族当前没有 serde 派生。

Lumen 需要自行映射：

- `Report.unmapped`
- `Report.warnings`
- `Severity::Silent`
- `Severity::Degraded`
- `Severity::Fatal`
- `UnmappedReason::*`

处理规则：

- `take_report()` 会取走并清空当前报告。
- `Strict` 模式下，`Fatal` 可能导致 `Result::Err`。
- 默认模式下降级应落日志/事件，不得静默吞掉。
- `Opaque` 只记录 `kind + len`，不得记录密文、签名或 encrypted reasoning 明文。

---

## 10. Capabilities 装配

Lumen 继续负责把模型表映射为 capability 策略：

- `thinking`
- `max_output_tokens`
- `supported: ParamSet`
- `passthrough_cache_control`
- `passthrough_betas`
- `Mode`

`llmwire` 只提供机制，不提供模型策略来源。

建议：

- 简单场景使用 `resolve(inbound, backend, model)`。
- 多模型覆盖使用 `StaticHost` / `ModelProfile` / 自定义 `Host`。
- 未知模型策略由 Lumen 明确配置，不依赖默认猜测。

---

## 11. Lumen 必须保留的职责

以下能力不应下沉到 `llmwire`：

- 路由解析与候选排序
- API key / 鉴权 header
- Base URL / 代理 / 超时
- 重试与降级
- 模型别名到上游模型名
- stream_options 注入
- 用量记账与计费
- 虚拟密钥、配额、预算
- 请求日志、会话捕获、DB 写入
- 响应头处理与客户端状态码
- 首字节后关闭语义

---

## 12. 已知限制

以下限制不是本次 P0 缺陷，Lumen 应显式处理：

| 能力 | 当前行为 | Lumen 处理 |
|---|---|---|
| Gemini | 不在 `llmwire` 内 | 保持透传或 Lumen 自行实现 |
| 有状态 Responses | 不支持 `previous_response_id` / `store:true` | 修改请求或显式拒绝 |
| Chat 流式 `n > 1` | 明确 `Unsupported` | 拒绝或降级为单候选，但不得静默 |
| 图片 `file_id` | 不支持 | 拒绝或 host 预解析 |
| 图片 URL / Base64 | 支持表示转换 | 不得下载 URL、不得上传 Base64 |
| Request streaming | 不支持 | Lumen 处理请求侧上传 |
| OpenAI 方言私有用量字段 | 未专门收敛 | Lumen 记账侧自行兼容 |
| `Report` serde | 未派生 | Lumen 手写映射 |

---

## 13. Lumen 接入验收清单

接入完成需要逐项打勾：

- [ ] 依赖固定到 `153d74a4da7df82a3c7a3b848d671429508fc8a2` 或确认后续 commit。
- [ ] 同协议端点仍字节透传，不构造 `Converter`。
- [ ] 跨协议 6 个有向组合，非流式文本转换正确。
- [ ] 跨协议 6 个有向组合，流式转换与 `Termination` 正确。
- [ ] 上游响应 `model` 与客户端请求 `model` 不同，客户端仍看到上游 `model`。
- [ ] Chat 流式首个及后续 chunk 的 `id` / `model` 一致。
- [ ] 请求转发前完成上游模型名 patch。
- [ ] 流式转发前完成 `include_usage` patch。
- [ ] tool call / tool result 跨协议往返，`tool_use.id` 字节保真。
- [ ] thinking / signature / encrypted reasoning 按策略透传或显式降级。
- [ ] OpenAI / Anthropic 缓存用量仍可记账，且符合 IR usage 包含关系。
- [ ] 上游 4xx / 5xx / 流中错误 / `Report::Fatal` 映射正确。
- [ ] 首字节发出后不再降级。
- [ ] 有状态 Responses、Gemini、`file_id`、Chat 流式 `n > 1` 有明确拒绝或旁路路径。

---

## 14. 推荐验证命令

在 `F:\llmwire` 执行：

```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked -p llmwire
cargo test --locked -p llmwire --test response_metadata
cargo test --locked -p llmwire --test robustness
pwsh -NoProfile -File scripts/check-core-deps.ps1
pwsh -NoProfile -File scripts/check-live-gate.ps1
```

本次移交前这些命令均已通过。

---

## 15. 交接结论

可以移交给 Lumen 开发接入。

建议在交接说明中明确：

1. 交付基线为 `153d74a4da7df82a3c7a3b848d671429508fc8a2`，P0 功能修复位于其父提交 `8340d27e389e2101445a87735d6780487b598132`。
2. Lumen 原 `llmwire对接文档.md` 仍是集成契约。
3. P0 响应 `model` 问题已修复，并同步修复 `id`。
4. 剩余风险在 Lumen-side E2E 接线与记账/降级策略，不在协议转换核。
5. 若进入正式发布，必须完成本文第 13 节验收清单。
