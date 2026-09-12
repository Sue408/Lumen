# 计划：24h 吞吐 TPS 与上游连通性

> 日期：2026-09-13
> 状态：已完成（2026-09-13）
> 关联：`db/telemetry.rs`（新增）、`gateway/probe.rs`（新增）、`commands/telemetry.rs`（新增）

## 目标

- 在 **Providers 页**展示近 24h 连通性：被动统计（成功率 / 延迟 / 最后成功）为主，手动「最小消息」探测为辅。
- 在 **Usage 页**展示近 24h 吞吐 TPS（全站吞吐）与模型生成速度 TPS。
- 不新建独立监控页，守住设计文档「不是控制台，不是仪表盘」的定位。

## 决策基线

- **TPS 双口径**：
  - 全站吞吐 = 窗口内 `Σtokens / 窗口秒数`（纯只读，无需 schema）。
  - 模型生成速度 = `Σoutput_tokens / Σ(latency_ms − ttfb_ms)`，仅取流式成功记录。
- **连通性**：被动聚合 + 手动探测，绝不后台轮询。
- **落点**：连通性 → Providers 页；TPS → Usage 页。

## 后端

### 1. 迁移 14：记录首字节时间

- `db/migrations.rs`：`SCHEMA_VERSION 13 → 14`；`SCHEMA` 的 `request_logs` 加 `ttfb_ms INTEGER`；`MIGRATIONS` 补 `ALTER TABLE request_logs ADD COLUMN ttfb_ms INTEGER;`
- `db/models.rs`：`RequestLog` 加 `ttfb_ms: Option<i64>` + `from_row` 读取。
- `db/logs.rs`：`insert_log` 列清单补 `ttfb_ms`。
- `gateway/forward.rs`：仅动 `stream_response`——`UsageScanner` 首个数据 chunk 到达时记 `first_chunk: Option<Instant>`，收尾时对 `build_log(...)` 返回值补 `log.ttfb_ms`。`LogContext` 保持不动，避免 `handlers.rs` 内多处字面量连带修改。
- 非流式不填：一次性 body 无法拆生成阶段。

### 2. `db/telemetry.rs`（纯只读聚合）

- `throughput(conn, hours)`：滑动窗口按整点分桶，每桶 `tokens/3600` → 吞吐 TPS；附窗口总请求数、总 tokens。
- `generation_speed(conn, hours)`：仅 `is_stream=1 AND latency_ms > ttfb_ms AND status='success'`，按模型聚合。
- `connectivity(conn, hours)`：按 `provider_id` / `upstream_model_id` 分组，产出 `total / success / error / successRate / avgLatencyMs / lastSuccessAt / lastErrorAt`。

### 3. 手动探测（最小消息请求）

- `gateway/probe.rs`：从 provider 取连接配置、从首个 enabled 模型取 `model_id`，构造临时 `ResolvedRoute`，复用 `forward::send` 发最小请求：
  - openai → `/chat/completions`，`{messages:[{role:"user",content:"ping"}], max_tokens:1}`
  - anthropic → `/messages`，`{max_tokens:1, messages:[{role:"user",content:"ping"}]}`
  - responses → `/responses`，`{input:"ping", max_tokens:1}`
  - gemini → `/models/{id}:generateContent`，`{contents:[{parts:[{text:"ping"}]}], generationConfig:{maxOutputTokens:1}}`
- **不写日志、不计费、不触发冷却**（手动探测不得误伤真实降级链）。

### 4. 命令

- `state.rs`：加 `cooling_snapshot() -> Vec<String>`。
- `commands/telemetry.rs`：`query_telemetry_cmd(hours)`、`test_provider_cmd(provider_id)`。
- `lib.rs` 的 `register_handlers!` 注册。

## 前端

### 5. Providers 页（连通性）

- `services/telemetry.ts`：`queryTelemetry()` / `testProvider(id)` + 浏览器 mock 回退。
- 接上 `ProvidersPage.tsx` 已占位的烧瓶按钮：内联 loading / 成功（延迟 ms）/ 失败（错误原文）。
- `register-meta` 扩为「N 个模型 · 近 24h 98% · 620ms」；在线点复用 `--live` 苔绿，失败 / 冷却 / 停用走朱砂。
- 复用 `useLiveRevision` 事件链触发重查。

### 6. Usage 页（TPS）

- 新增 `features/usage/ThroughputPanel.tsx`，独立区块挂在 `metric-strip` 之后，标题「近 24 小时」。
- 吞吐曲线沿用 `UsageTrendChart` 的去脚手架绘制。
- 生成速度以文字行呈现（「流式生成 42 tok/s · 仅近期流式请求」）。
- 不并入现有 `metric-strip`：自然日/周/月周期与 24h 滑动窗口语义不同。

## 测试与验证

- 后端：telemetry 聚合单测、probe 各协议 body 单测、迁移 14 数据保留测试。
- 前端：TPS / TTFB 格式化纯逻辑 `.test.ts`。
- `pnpm test`、`pnpm build`、`cargo test`、`cargo clippy -- -D warnings` 全绿。

## 落地顺序

1. 迁移 14 + `ttfb_ms` 写入。
2. `db/telemetry.rs` + 命令 + 单测。
3. Usage 页 TPS 区块。
4. Providers 页连通性 + 手动探测。
5. 样式齐备与设计边界复核。

## 风险与边界

- 历史记录无 `ttfb_ms` → 生成速度初期样本少，UI 明示口径。
- 生成速度口径 = `latency_ms − ttfb_ms`（纯生成阶段），非完整请求耗时。
- 探测仅手动、绝不后台轮询；绝不写账本、绝不触发冷却。
- 不建独立监控页。
