# 网关错误归因与可定位性 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让每条失败调用在日志里**可归因、可细分、可追因**——把「上游提供商 / 网关 / 客户端」三方职责从本质处分开，把链路（transport）作为「去上游那一跳」的限定词，并补齐当前流水盲区与假成功。

**Architecture:** 在 `error.rs` 定义归因词汇表（`ErrorDomain` / `ErrorKind` / `LogError`），`request_logs` 增量扩三列（`error_domain` / `error_kind` / `trace_id`），`gateway/usage.rs` 的 `LogContext` 用 `Option<LogError>` 取代裸 `error_message`，所有网关失败点改填归因。冷却表由「只有截止时刻」升级为「带原因」，并加**全局性抑制**：同一客户端请求里跨多个 provider 同时出现链路失败时，判定为本机出口问题，不冷却任何候选。客户端断开改为独立 `cancelled` 状态，不再计入失败率。最后用 `trace_id` 把一次客户端调用的多次尝试串成一条链。

**Tech Stack:** Rust（axum / reqwest / rusqlite / chrono）、React 19 + TypeScript、Tauri 2。

**Spec:** `docs/后端接口文档.md`（`request_logs` DTO）；`docs/superpowers/plans/2026-09-14-llmwire-integration.md`（转换分支的失败语义沿用）；`docs/UIUX设计文档`（朱砂只用于失败 / 已停用）。

## Global Constraints

- 网关层（`gateway/`）不依赖 Tauri；持久化实体只在 `db/models.rs` 定义一处。
- **DB 只允许增量迁移**：`db/migrations.rs` 的 `SCHEMA` 同步新库建表、`SCHEMA_VERSION` +1、`MIGRATIONS` 补一条 `ALTER TABLE ... ADD COLUMN`。**严禁 DROP**；`request_logs` 与 `settings` 不得随升级丢失，必须配旧的库升级测试。
- 颜色仍只来自 `src/styles/theme.css`；本计划不新增颜色、不改布局尺寸。
- 前端纯计算逻辑独立成不依赖 React 的模块，与被测模块同目录配 `<module>.test.ts`（`node --test` 可跑）。
- 验证命令（每任务结束都跑）：
  - 后端：`cargo test`、`cargo clippy -- -D warnings`（于 `src-tauri/`）
  - 前端：`pnpm test`、`pnpm build`（根目录）

## 归因契约（唯一契约，后端落库 + 前端展示共用）

### 三方 + 链路限定词

- `ErrorDomain` ∈ `upstream` / `gateway` / `client`——**谁**的问题。
- `ErrorKind`（snake_case，前缀分组）——**哪一类**的问题。
- 链路（DNS / connect / TLS / 代理 / 超时 / 流中断）**不是第四方**：它归 `domain = upstream`，但用 `link_*` 类目标明「问题出在路上、不在对面」，以便冷却与文案区分。
- 第三方既定事实：腿 A（客户端 ↔ 网关）恒为 `127.0.0.1` 回环（`gateway/mod.rs` 只绑回环），因此客户端侧的断开 / 超时是**行为**，不得标注为网络。

### 归因映射表（本计划的验收基准）

| 场景 | domain | kind | 冷却 | 降级 | 失败率 | 备注 |
|---|---|---|---|---|---|---|
| 虚拟密钥缺失 / 未知 / 停用 | client | `unauthorized` | ✗ | ✗ | ✓ | 现状已落流水 |
| 请求缺 `model` | client | `invalid_request` | ✗ | ✗ | ✓ | **现状盲区** |
| body 超限 | client | `payload_too_large` | ✗ | ✗ | ✓ | **现状盲区**（`gateway/body.rs`） |
| JSON 畸形 / 非 json content-type | client | `malformed_body` | ✗ | ✗ | ✓ | **现状盲区** |
| 未知端点（`base_url` 写错） | client | `unknown_endpoint` | ✗ | ✗ | ✓ | **现状只进运行日志** |
| 客户端中断连接（流式） | client | `client_closed` | ✗ | ✗ | **✗** | 状态改 `cancelled`，单列 |
| 别名不存在 / 路由未启用 | gateway | `route_not_found` | ✗ | ✗ | ✓ | |
| 别名在但目标 / 模型 / 端点全不可用 | gateway | `route_no_candidate` | ✗ | ✗ | ✓ | **与上条同判 404，须拆开** |
| 全部候选处于冷却 | gateway | `all_candidates_cooling` | ✗ | ✗ | ✓ | 带每个候选的冷却快照 |
| 本机出口不可达（跨 provider 链路齐失败） | gateway | `egress_unreachable` | ✗（抑制） | ✗ | ✓ | **新增全局性抑制** |
| 额度超限 | gateway | `quota_exceeded` | ✗ | ✗ | ✓ | |
| 跨协议语义不支持 | gateway | `unsupported_conversion` | ✗ | ✗ | ✓ | |
| 协议转换失败 / Fatal | gateway | `conversion_failed` | ✗ | ✓ | ✓ | |
| DB / 锁 / 内部错误 | gateway | `storage_error` / `internal_error` | ✗ | ✗ | ✓ | **resolve 失败现状盲区** |
| 上游 4xx（非 429） | upstream | `upstream_rejected` | ✗ | ✗ | ✓ | 透传状态与原文 |
| 上游 401 / 403 | upstream | `upstream_auth_failed` | ✗ | ✗ | ✓ | 多为 provider key 配置 |
| 上游 429 | upstream | `upstream_rate_limited` | ✓ | ✓ | ✓ | 短 `Retry-After` 先重试同候选 |
| 上游 5xx（含 529） | upstream | `upstream_unavailable` | ✓ | ✓ | ✓ | |
| 上游对流式请求回非 SSE | upstream | `upstream_stream_mismatch` | ✓ | ✓ | ✓ | 文案提示查 `base_url` |
| 上游 2xx 却是错误体 | upstream | `upstream_bad_response` | ✓ | ✓ | ✓ | **现状假成功** |
| 上游流未正常收尾 | upstream | `upstream_truncated` | ✗ | ✗ | ✓ | **现状假成功** |
| 链路：DNS / 连接 / TLS / 代理失败 | upstream | `link_connect` | ✓（可抑制） | ✓ | ✓ | reqwest 无法再细分，标注局限 |
| 链路：等响应头超时 | upstream | `link_timeout` | ✓（可抑制） | ✓ | ✓ | 非流式 600s |
| 链路：流中途 reset / 断网 | upstream | `link_stream_reset` | ✗ | ✗ | ✓ | 已发字节，不可降级 |
| 链路：流静默超时 | upstream | `link_stream_stalled` | ✗ | ✗ | ✓ | 块间 300s |

### 冷却抑制规则（新增）

- 单个候选的链路失败 → 仍按瞬时失败冷却（60s），因为可能是该 provider 独有。
- **同一客户端请求**里，**≥ 2 个不同 provider** 的候选都因 `link_connect` / `link_timeout` 失败 → 判定**本机出口问题**：撤销本次请求已排队的冷却，整体归因 `gateway` / `egress_unreachable`，返回带中文提示的 502。
- 抑制只作用于**本次请求**内累计的待冷却项；不追溯、不清空已有冷却。

## 设计决议（已拍板）

- **链路不作为第四方**：`domain` 保持三方，链路用 `link_*` 类目表达，避免「网络」变成一个无法追责的垃圾桶。
- **`error_message` 保留为人话**：新增 `error_domain` / `error_kind` 供机读与筛选，`error_message` 退回「可读诊断」，不再承载分类语义。
- **`trace_id` 是 Lumen 侧的一次客户端请求标识**：与上游回填的 `request_id` 分离；连接失败时 `request_id` 为 `None`，但 `trace_id` 恒在。
- **客户端取消用 `status = "cancelled"`**，不新增 `error_kind` 之外的失败维度；失败率与「待处理」范围剔除 cancelled。
- **不新增 `error_phase` 列**：阶段信息编码进 `link_*` / `upstream_truncated` 等 kind，避免过早加列（后续确有需要再增量迁移）。
- **不追责上游 401/403 与 Lumen key 配置**：客户端密钥错由 Lumen 鉴权在本地拦下（`unauthorized`）；透传的上游 401/403 记 `upstream_auth_failed`，文案提示多为 provider 侧凭据配置。

## 非目标（明确不做）

- 不改定价、不改用量口径、不改 `usage_source` 语义。
- 不改同协议字节透传路径与 llmwire 集成契约。
- 不重构协议转换核；不引入按模型的能力策略表。
- 不为失败请求做自动重放；不做跨请求的持久化冷却。
- 不重做日志页布局；只在详情里补归因字段与尝试序号。

---

### Task 1: 归因契约（枚举 + reqwest 分类）

**Files:**
- Modify: `src-tauri/src/error.rs`

- [x] 定义 `pub enum ErrorDomain { Upstream, Gateway, Client }`，`as_str()` 返回 `upstream` / `gateway` / `client`。
- [x] 定义 `pub enum ErrorKind`，按上表落地全部类目，`as_str()` 返回 snake_case、`domain()` 返回所属 `ErrorDomain`；另加 `is_link()` 供冷却抑制判定。
- [x] 定义 `pub struct LogError { pub kind: ErrorKind, pub message: String }`。**决议偏差**：归属由 `kind` 唯一决定（`kind.domain()`），故只提供 `LogError::new(kind, msg)`，不设 `upstream` / `gateway` / `client` 三个构造器，避免构造出不一致的组合。
- [x] `pub fn classify_reqwest(error) -> (ErrorKind, &'static str)`：抽出纯函数 `classify_transport` 做 `is_timeout` / `is_connect` / `is_request` / `is_body|is_decode` 映射（timeout 优先），返回中文诊断。DNS / TLS / 代理统一落 `LinkConnect` 并标注局限。
- [x] `impl AppError`：新增 `pub fn log_error(&self) -> LogError`，映射 `Unauthorized` / `ModelNotFound` / `QuotaExceeded` / `Db` / `Io|Serde|Join` / `Http` / 其余。
- [x] 单测：`as_str` + `domain()` 全表稳定性、`is_link` 判定、`classify_transport` 穷举、`AppError::log_error` 映射矩阵。
- [x] 归因词表在 Task 3～8 接线前加临时 `#[allow(dead_code)]`，Task 9 清理。
- Verify: `cargo test`、`cargo clippy -- -D warnings`。

### Task 2: `request_logs` 扩容与 DTO

**Files:**
- Modify: `src-tauri/src/db/migrations.rs`
- Modify: `src-tauri/src/db/models.rs`
- Modify: `src-tauri/src/db/logs.rs`
- Modify: `src/services/gateway.ts`
- Modify: `src/features/logs/logQuery.ts`

- [x] 迁移：`SCHEMA` 建表补 `error_domain TEXT` / `error_kind TEXT` / `trace_id TEXT`；`SCHEMA_VERSION` 16 → 17；`MIGRATIONS` 补 v17 `ADD COLUMN` 三条；`SCHEMA` 补 `idx_request_logs_trace`。
- [x] `RequestLog` 增 `error_domain` / `error_kind` / `trace_id`，`from_row` 同步读取；全部结构体字面量（demo / keys / stats / telemetry / logs 测试 / `build_log`）补齐。
- [x] `db/logs.rs` 的 INSERT 扩到 33 列；`LogFilter` 增 `error_domain` / `error_kind` 等值过滤。
- [x] 迁移测试：新增 `downgrade_logs_to_pre_v17` 辅助，两个升级测试都先撤 v17 结构；`migrates_without_losing_rows` 断言旧行三列均为 `NULL`（`settings` 保留由既有测试覆盖）。
- [x] 前端 `services/gateway.ts` 的 `RequestLog` 补 `errorDomain` / `errorKind` / `traceId`；`services/usage.ts` 的 `LogFilter` 补同名字段；`logQuery.ts` 的 `buildLogFilter` 支持 `errorDomain` 下推（UI 分类键留到 Task 5 / 9）。
- Verify: `cargo test`、`cargo clippy -- -D warnings`、`pnpm test`、`pnpm build`。

### Task 3: `LogContext` 收敛为 `LogError`

**Files:**
- Modify: `src-tauri/src/gateway/usage.rs`
- Modify: `src-tauri/src/gateway/handlers.rs`

- [x] `LogContext` 的 `error_message: Option<String>` 改为 `error: Option<LogError>`；`build_log` 从中派生 `error_domain` / `error_kind` / `error_message`。
- [x] 全部 `LogContext` 构造点改填归因：`handlers.rs` 11 处、`forward.rs` 1 处、`reject.rs` 1 处；对照归因映射表落 domain 与 kind。
- [x] 上游非 2xx 保留 `error_message(&value, &text)` 的人话，外层用 `error.rs::classify_upstream_status(status)` 包上 `upstream_*` 四类 kind（流式首字节前与非流式共用；401/403 单列为 `upstream_auth_failed`）。
- [x] `reject.rs` 新增 `reject_with(status, LogError)`，`reject` 委托给它；`forward.rs` 流式收尾的 `failure` 由 `Option<String>` 改为 `Option<LogError>`，流中 reqwest 错误走 `classify_reqwest`。`probe.rs` 不落库，维持原诊断文案（**偏差**：未改）。
- [x] 消歧：新增 `db::routes::enabled_alias_exists`，`forward` 里把空候选拆成 `route_not_found`（别名不存在）与 `route_no_candidate`（别名在但无可用候选）。
- [x] 单测：鉴权 → `client/unauthorized`；额度 → `gateway/quota_exceeded`；未知别名 → `gateway/route_not_found`；新增「别名在但目标全停用」→ `gateway/route_no_candidate`；客户端断开 → `client/client_closed`。
- Verify: `cargo test`、`cargo clippy -- -D warnings`。

### Task 4: 冷却带原因 + 全局性抑制

**Files:**
- Modify: `src-tauri/src/state.rs`
- Modify: `src-tauri/src/gateway/handlers.rs`
- Modify: `src-tauri/src/commands/telemetry.rs`

- [x] `Cooldowns` 升级为 `HashMap<String, Cooling { provider_id, kind, until }>`；`mark_cooling(id, provider_id, kind, duration)` 增原因参数。**偏差**：未保留 `since`（无人消费，属过早字段）。
- [x] `cooling_snapshot() -> Vec<CoolingView>`（id / provider / kind / 剩余秒数）；`TelemetryDto` 保留既有 `cooling: Vec<String>` 以兼容前端 Set，另加 `coolingDetail`（前端暂不展示）。
- [x] `forward` 失败当刻只压 `Vec<PendingCooldown>`；成功 / 透传返回时自然丢弃。
- [x] 候选耗尽后统一结算：`link` 类失败涉及 **≥ 2 个不同 provider** → 丢弃全部待冷却、记 `gateway` / `egress_unreachable`、回中文 502；否则逐条落盘冷却并透传最后一次错误。
- [x] 「全部候选冷却」日志改为从冷却快照取被过滤候选的原因与剩余时长，`route` 填首个被冷却候选（详情不再显示「未匹配到路由」）。
- [x] 单测：`state.rs` 冷却快照 / 过期 / 原因；handlers —— 全冷却带快照与剩余、单 provider 链路失败仍冷却、跨 provider 链路齐失败归 `egress_unreachable` 且不冷却。
- Verify: `cargo test`、`cargo clippy -- -D warnings`。

### Task 5: 客户端取消与统计口径

**Files:**
- Modify: `src-tauri/src/gateway/forward.rs`
- Modify: `src-tauri/src/db/stats.rs`
- Modify: `src-tauri/src/db/logs.rs`
- Modify: `src/features/logs/logQuery.ts`
- Modify: `src/features/logs/LogsPage.tsx`

- [x] `stream_response` 下游断开：`status = "cancelled"`，归因 `client` / `client_closed`，`error_message` 带「已发送 N 字节」（扫描任务累计）。
- [x] 失败率与「失败 / 待处理」剔除 cancelled：经核对，`db/stats.rs` 与 `db/telemetry.rs` 均已按 `status = 'error'` 计数、`attentionOnly` 也用 `status = 'error'`，cancelled 天然被排除，无需改动（**偏差**：无后端口径改动）。
- [x] 前端 `logQuery.ts`：`describeLog` 对 `cancelled` 返回中性 mark（「已取消」），新增 `cancelled` 范围与 `status = "cancelled"` 过滤；`logs.css` 增设中性左规线，未动既有类名序列。
- [x] `logQuery.test.ts`：`cancelled` 为独立中性范围、不进 `failed`；`handlers` 的客户端断开测试改断言 `status = "cancelled"` 与「已发送」字节数。
- Verify: `cargo test`、`cargo clippy -- -D warnings`、`pnpm test`、`pnpm build`。

### Task 6: 补齐流水盲区

**Files:**
- Modify: `src-tauri/src/gateway/handlers.rs`
- Modify: `src-tauri/src/gateway/body.rs`
- Modify: `src-tauri/src/gateway/mod.rs`

- [x] 缺 `model`：先 `record_log`（`client` / `invalid_request`，400）再返回；`is_stream` 提前计算以便归因。
- [x] `JsonBody` 提取失败：把提取器实现收敛为 `FromRequest<Arc<AppState>>`，失败时落 `client` 日志（`payload_too_large` / `malformed_body`），无别名与 key（`build_log` 把空别名归一为 `NULL`）。**偏差**：未走 `log_request` 中间件方案。
- [x] 未知端点 `not_found`：改为接收 `State`，补记 `client` / `unknown_endpoint`；运行日志 `tracing::warn` 保留。
- [x] `resolve_all` 失败：补记（`reject` → `gateway` / `storage_error`）后再返回。
- [x] 非流式响应体读取失败：补记 `error.log_error()`（多为 `upstream` / `link_stream_reset`）。
- [x] 验证：新增缺 `model`（`invalid_request`）、未知端点（`unknown_endpoint`）handlers 测试；`body.rs` 两条测试改为经真实 `AppState` 落库并断言 `payload_too_large` / `malformed_body`。
- Verify: `cargo test`、`cargo clippy -- -D warnings`。

### Task 7: 假成功检测

**Files:**
- Modify: `src-tauri/src/gateway/handlers.rs`
- Modify: `src-tauri/src/gateway/forward.rs`

- [x] 非流式 2xx 带顶层 `error`（非 null / 非空对象 / 非空串）：判 `upstream_bad_response`，`status = "error"`、用量 `missing`、冷却 + 降级；最后候选时透传 502 结构化错误。
- [x] 流式收尾：`UsageScanner` 暴露 `finalized()`；非转换路径自然结束但 `!finalized` 时判 `upstream_truncated`。转换路径仍以 `Termination` 为准。**已知局限**已写入实现注释与本文档。
- [x] 单测：200 + `{"error":{...}}` → `upstream_bad_response` 且 `usage_source = missing`；无终止符 SSE → `upstream_truncated`；既有 `[DONE]` 流式成功测试保持绿。
- Verify: `cargo test`、`cargo clippy -- -D warnings`。

### Task 8: `trace_id` 贯穿与尝试链展示

**Files:**
- Modify: `src-tauri/src/gateway/handlers.rs`
- Modify: `src-tauri/src/gateway/forward.rs`
- Modify: `src-tauri/src/gateway/usage.rs`
- Modify: `src/features/logs/logQuery.ts`
- Modify: `src/features/logs/LogsPage.tsx`

- [x] `forward` 入口生成 `trace_id`（`uuid` v4），写入本请求每一条 `LogContext` / `StreamMeta` / `reject`；`build_log` 落 `trace_id`。
- [x] 详情新增「追踪」（`traceId`）与「尝试」（第 N 次）两行。**偏差**：列表未做按 trace 折叠 / 「共 N 次」标识，只提供纯函数与详情展示。
- [x] `logQuery.ts` 新增 `groupByTrace(logs)` 纯函数 + 单测（无 trace 各自独立、同 trace 按 `attemptIndex` 升序）。
- [x] 后端测试：失败 + 成功两次尝试共享同一 `trace_id`，`attempt_index` 为 0 / 1。响应头 `x-lumen-trace-id` **未做**（避免动透传字节路径）。
- Verify: `cargo test`、`cargo clippy -- -D warnings`、`pnpm test`、`pnpm build`。

### Task 9: 文档同步与验收

**Files:**
- Modify: `docs/后端接口文档.md`
- Modify: `docs/UIUX设计文档`

- [x] `docs/后端接口文档.md`：`request_logs` DTO 补 `errorDomain` / `errorKind` / `traceId`、`status` 含 `cancelled`、`LogFilter` 补归因过滤；补归因词表与「链路归 upstream」口径；迁移节补 v17。
- [x] `docs/UIUX设计文档`：使用日志节补归因三方、`cancelled` 中性（非朱砂、不计失败率）、`traceId` / 尝试序号展示、新增「已取消」口径。
- [x] 核对归因映射表：每条 kind 均有落库断言或实现出处；TLS / DNS 无法细分已在代码与文档标注为**已知局限**。
- [ ] 端到端核对（GUI 目视，待人工）：dev 库构造错误密钥 / 全候选冷却 / 客户端断开 / 连接失败，确认归因与筛选范围符合预期。
- [x] 清理 Task 1 的临时 `#[allow(dead_code)]`：词表全部被消费后已移除，`cargo clippy -- -D warnings` 保持干净。
- Verify: `pnpm test`、`pnpm build`、`cargo test`、`cargo clippy -- -D warnings`。

## 风险与注意

- **迁移只准增量**：本计划只 `ADD COLUMN` + 建索引，不触碰既有列与数据；旧的 `request_logs` 行新列为 `NULL`，前端需容忍空值。
- **历史行没有归因**：老数据 `error_domain` / `error_kind` 为空，筛选与统计需把 `NULL` 视为「未归因」，不得折算成 `gateway`。
- **`LogContext` 改动面广**：13 处构造点（`handlers.rs` 11、`forward.rs` 1、`reject.rs` 1）一次改完，漏改会编译失败（结构体字段变更），属可接受的强约束。
- **假成功检测的误报**：非标准收尾的上游会被 `upstream_truncated` 误伤；上线前需用现有 mock 与真实 provider 各验证一次，必要时按 provider 加豁免。
- **冷却抑制的边界**：阈值为「≥ 2 个不同 provider」是启发式；单 provider 多候选同时链路失败仍按普通冷却处理，避免把 provider 故障误判为本地问题。
- **`cancelled` 影响既有统计断言**：`db/stats.rs` 与前端 `logQuery.test.ts` 的失败口径断言需同步更新，改前先确认没有别处依赖 `status = 'error'` 的全集。
