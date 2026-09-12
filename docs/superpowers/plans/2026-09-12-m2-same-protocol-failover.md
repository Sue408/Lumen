# M2 同协议降级 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让一条路由的多目标真正成为**有序降级链**：`resolve` 返回有序候选，`forward` 按 attempt 循环，仅在连接失败 / 超时 / 5xx / 429 时切换；每次尝试都进账（`attempt_index` + 实际履约 target），守住「账不低报」红线。

**Architecture:** `resolve` 由 `LIMIT 1` 改为返回按 `priority` 排序的候选 `Vec<ResolvedRoute>`。失败分类与冷却做成纯逻辑模块（`gateway/failover.rs`），`forward` 持 attempt 循环：逐候选尝试，2xx 即履约；429 按 `Retry-After` 决定「等一次重试同目标」或「降级」；5xx / 连接失败 / 超时降级；其余 4xx 直接透传不降级。每次尝试写一条 `request_logs`（`attempt_index` 递增、记实际 target）。冷却态放 `AppState` 内存。流式只允许在首字节前切换。

**Tech Stack:** Rust（rusqlite 0.32 / axum / reqwest）、Tauri 2、React 19、TypeScript、Vite

**Spec:** `docs/后续功能计划备忘.md` §3（路由调用策略）/ §5 M2；`docs/后端接口文档.md`

**依赖:** M1 全部完成（迁移框架、`route_targets` upsert 使 target `id` 稳定、dev/release 隔离）。

## 设计决议（已拍板）

- **选择策略只保留优先级**：按 `priority` 升序；默认**粘性**（主目标可用就不换）。不做轮询 / 加权 / 延迟优先 / 对冲（§3.1）。
- **失败触发**：连接失败 / 超时 / 5xx / 429。4xx（429 除外）、鉴权失败、内容策略**不**降级（§3.2）。
- **429 分类**：有短 `Retry-After`（等待上限 `MAX_RETRY_AFTER = 10s`）→ 等一次、**重试同一目标**；额度类 / 长冷却 / 无信号 → **降级**。
- **冷却**：每目标一个 `cooldown_until`，**内存态**、带过期；连续 429 作为「不需语义」的兜底跳过。
- **记账**：每次 attempt 一条日志，`attempt_index` 从 0 递增，记**实际履约 target**（§3.3）。红线优先于一切可选功能。
- **流式**：首字节后不降级；只可能在「建连 + 首块到达前」切换（§3.2）。
- **主动预算不进路由决策**（§3.4）。
- **候选耗尽**：透传**最后一次**尝试的错误响应（保留上游状态码与 body）；无响应可用时回 502。

## Global Constraints

- 网关层（`gateway/`）不依赖 Tauri；失败分类、`Retry-After` 解析、冷却判定等纯逻辑独立成可单测模块。
- 每次尝试都必须 `record()` 落库并发事件；金额口径不变，仍走 `gateway/usage.rs` 的纯函数。
- 数据结构仍在 `db/models.rs` 定义一处；迁移只在 `db/migrations.rs`，**只允许增量**。
- 接口 / 行为契约变更后同步 `docs/后端接口文档.md`。
- 验证命令（每任务结束都跑）：`cargo test`、`cargo clippy -- -D warnings`（于 `src-tauri/`）；涉及前端时加 `pnpm test`、`pnpm build`。

## 非目标（明确不做）

- 不做跨协议降级（属 M3；本里程碑所有候选与路由同协议）。
- 不做轮询 / 加权 / 延迟优先 / 对冲 / 主动预算路由。
- 不做冷却状态持久化（重启清空可接受）。
- 不做对冲式并行双发。
- 不把 `attempt_index` 暴露成可配置项，也不改日志页 UI（分组展示后置）。

---

### Task 1: 记账字段 `request_logs.attempt_index`

**Files:**
- Modify: `src-tauri/src/db/migrations.rs`
- Modify: `src-tauri/src/db/models.rs`
- Modify: `src-tauri/src/db/logs.rs`
- Modify: `src-tauri/src/gateway/usage.rs`（`LogContext` / `build_log`）
- Modify: `src/services/gateway.ts`（`RequestLog` 类型）
- Modify: `src/services/usage.ts`（mock 生成器）

- [ ] `migrations.rs`：`SCHEMA_VERSION` 8 → 9；`SCHEMA` 的 `request_logs` 加列 `attempt_index INTEGER NOT NULL DEFAULT 0`；`MIGRATIONS` 追加：
      ```sql
      ALTER TABLE request_logs ADD COLUMN attempt_index INTEGER NOT NULL DEFAULT 0;
      ```
- [ ] `models.rs`：`RequestLog` 增加 `pub attempt_index: i64`；`from_row` 读 `attempt_index`。DTO 走 `camelCase`，前端得到 `attemptIndex`。
- [ ] `db/logs.rs`：`insert_log` 的列清单与绑定参数补 `attempt_index`。
- [ ] `gateway/usage.rs`：`LogContext` 增加 `attempt_index: i64`；`build_log` 透传。所有既有构造点（`handlers.rs`、`forward.rs`、`reject.rs`）先补 `attempt_index: 0`。
- [ ] 前端 `src/services/gateway.ts` 的 `RequestLog` 增加 `attemptIndex: number`；`src/services/usage.ts` mock 行补 `attemptIndex: 0`。
- [ ] 写测试（`db/migrations.rs`）：
      - 现有 `migrates_without_losing_rows` 扩展到断言 `request_logs` 行保留且新列默认 0。
      - 新库 `request_logs` 含 `attempt_index` 列。
- [ ] `cargo test`、`cargo clippy -- -D warnings`、`pnpm test`、`pnpm build`。

### Task 2: `resolve` 返回有序候选

**Files:**
- Modify: `src-tauri/src/gateway/resolve.rs`
- Modify: `src-tauri/src/gateway/handlers.rs`（调用点）

- [ ] `resolve.rs`：新增 `pub fn resolve_candidates(conn, alias) -> Result<Vec<ResolvedRoute>, AppError>`：去掉 `LIMIT 1`，保留 `ORDER BY t.priority ASC, t.rowid ASC`，收集全部行。
- [ ] 保留 `resolve_route`（返回首个）作为 `resolve_candidates` 的薄封装，避免既有测试 / 调用点大改。
- [ ] 新增 `pub async fn resolve_all(state, alias) -> Result<Vec<ResolvedRoute>, AppError>`；`handlers::forward` 改用 `resolve_all`。
- [ ] 写测试（`resolve.rs`）：
      - `resolves_candidates_in_priority_order`：两个目标（priority 1、0）→ 返回顺序为 0 在前。
      - `skips_disabled_targets`：禁用一个目标 → 结果不含它。
      - `route_without_enabled_targets_is_empty`：全部禁用 → `Ok(vec![])`。
      - 既有 `resolves_enabled_alias_to_upstream` 保持通过。
- [ ] `cargo test`、`cargo clippy -- -D warnings`。

### Task 3: 失败分类纯逻辑

**Files:**
- Create: `src-tauri/src/gateway/failover.rs`
- Modify: `src-tauri/src/gateway/mod.rs`（`pub mod failover;`）

- [ ] 定义：
      ```rust
      pub const MAX_RETRY_AFTER: Duration = Duration::from_secs(10);

      #[derive(Debug, Clone, Copy, PartialEq, Eq)]
      pub enum AttemptAction {
          /// 2xx：本次候选履约，停止切换。
          Served,
          /// 429 + 短 Retry-After：等待后重试同一目标。
          RetrySame { delay: Duration },
          /// 可恢复失败：换下一个候选。
          Failover,
          /// 不可恢复：透传错误，不切换。
          GiveUp,
      }

      pub fn classify(status: StatusCode, retry_after: Option<Duration>) -> AttemptAction;
      ```
      规则：2xx → `Served`；429 → `retry_after <= MAX_RETRY_AFTER` 则 `RetrySame`，否则 `Failover`；5xx（含 529）→ `Failover`；其余 4xx → `GiveUp`。
- [ ] 提供 `pub fn parse_retry_after(headers: &HeaderMap) -> Option<Duration>`：解析秒数（数字）；HTTP-date 形式解析失败即视为无信号（→ 降级）。
- [ ] 可选：`pub fn is_quota_exhaustion(body: &Value) -> bool`（best-effort 识别 `insufficient_quota` 等），仅用于冷却时长加权，不改变 `Failover` 判定。
- [ ] 写测试：`200→Served`；`500→Failover`；`503→Failover`；`529→Failover`；`429 + 2s→RetrySame(2s)`；`429 + 60s→Failover`；`429 无头→Failover`；`400/401/403/404→GiveUp`；`parse_retry_after` 对 `"2"` 与非法值的行为。
- [ ] `cargo test`、`cargo clippy -- -D warnings`。

### Task 4: 目标冷却登记（内存态）

**Files:**
- Modify: `src-tauri/src/state.rs`
- Test: `src-tauri/src/state.rs`

- [ ] `AppState` 增加 `cooldowns: Mutex<HashMap<String, Instant>>`（键为 `upstream_model_id`），并实现：
      - `pub fn is_cooling(&self, upstream_model_id: &str) -> bool`（读取时顺带清理过期项）。
      - `pub fn mark_cooling(&self, upstream_model_id: &str, duration: Duration)`。
- [ ] 冷却时长常量：5xx / 连接失败 → `COOLDOWN_TRANSIENT = 60s`；额度耗尽 → `COOLDOWN_EXHAUSTED = 300s`。
- [ ] 构造函数 `AppState::new` 初始化空 map（不破坏既有签名）。
- [ ] 写测试：`marks_and_reads_cooling`；`expired_cooldown_is_pruned`（注入过去时刻）。
- [ ] `cargo test`、`cargo clippy -- -D warnings`。

### Task 5: `forward` attempt 循环（非流式）

**Files:**
- Modify: `src-tauri/src/gateway/handlers.rs`
- Test: `src-tauri/src/gateway/handlers.rs`

- [ ] 重构 `forward`：
      1. `resolve_all` 取候选；空 → `ModelNotFound`（与现状一致）。
      2. 路由级协议校验（`required_protocol` / 路由与上游一致性）用 `candidates[0]` 做，失败即 `reject` 返回（沿用现逻辑）。
      3. 过滤 `state.is_cooling(id)` 的候选；若全部冷却 → 回 502 并记一条 `error`。
      4. `attempt_index` 从 0 起，逐候选循环：
         - 改写 body（`model` / `ensure_include_usage`）后 `send`。
         - `send` 返回 `Err`（连接失败 / 超时）→ 记 error attempt（`http_status: None`）→ `mark_cooling(transient)` → 下一候选。
         - 拿到 `response`：非流式读 `status` + `text`；`classify(status, parse_retry_after(headers))`：
           - `Served` → 记 success attempt（真实 usage / cost）→ 返回响应。
           - `RetrySame { delay }` → `tokio::time::sleep(delay)` 后**对同一候选重试一次**；仍非 `Served` 则按 `Failover` 处理。
           - `Failover` → 记 error attempt（若 body 有 usage 则照记，通常为 0）→ `mark_cooling` → 下一候选。
           - `GiveUp` → 记 error attempt → **直接返回上游错误响应**，不再切换。
      5. 候选耗尽 → 透传最后一次的错误响应（无则 502）。
- [ ] 保持鉴权与额度闸门顺序不变（在 `resolve` 之前）。
- [ ] 每次 `record()` 都必须带上递增的 `attempt_index` 与实际 `route`。
- [ ] 写测试：
      - `fails_over_on_5xx_and_serves_from_next`：候选 A 返回 500、候选 B 返回 200 → 客户端得 200；日志两条（attempt 0 error / attempt 1 success）；成功那条 cost 与手工核对一致。
      - `does_not_failover_on_4xx`：候选 A 返回 400 → 客户端得 400，仅一条日志，B 未被调用。
      - `retries_same_target_on_short_retry_after`：A 首次 429+`Retry-After: 1`、第二次 200 → 一条 attempt 内的重试后成功（同 target）。
      - `all_candidates_cooling_returns_502`。
- [ ] `cargo test`、`cargo clippy -- -D warnings`。

### Task 6: 流式 pre-first-byte 降级

**Files:**
- Modify: `src-tauri/src/gateway/handlers.rs`
- Test: `src-tauri/src/gateway/handlers.rs`

- [ ] 在 attempt 循环内，流式路径在调用 `stream_response` **之前**判状态：
      - `status` 非 2xx → 读错误 body 分类（与 Task 5 同规则）→ `Failover` 则记 error attempt、`mark_cooling`、下一候选。
      - `status` 2xx → 调用 `stream_response` 并返回；**此后不再降级**（首字节已可能发出）。
- [ ] 明确并注释：上游「200 开流后在流内报错」的情况不降级，只能透传。
- [ ] 写测试：
      - `stream_fails_over_before_first_byte`：A 返回 500、B 返回 200 SSE → 客户端得 200 且内容来自 B；两条日志。
      - 既有流式测试保持通过。
- [ ] `cargo test`、`cargo clippy -- -D warnings`。

### Task 7: 前端：目标列表去箭头 + 保存期告警

**Files:**
- Modify: `src/features/routing/RoutingPage.tsx`
- Modify: `src/styles/features/config.css`
- Modify: `src/features/routing/`（保存校验所在模块与组件）
- Test: 同目录 `<module>.test.ts`

- [ ] **移除连接箭头**：删掉 `RoutingPage.tsx` 中 `index > 0` 分支的 `<li className="target-connector" aria-hidden="true">↓</li>`；`Fragment` 包装随之退化为直接给 `<li key={target.uid}>`。
- [ ] 删除 `config.css` 的 `.target-connector` 规则（不留死 CSS）。
- [ ] 保留 `target-rank`（1/2/3 圆标）与 `target-role`（首选 / 备用 N）——顺序语义由它们承担。
- [ ] 确认 DOM 结构与类名序列的其余部分不变（AGENTS.md 要求），既有路由页测试通过。
- [ ] 新增纯函数 `hasUsableBackup(targets): boolean`（启用目标数 ≥ 2），并配测试。
- [ ] 保存路由时，若 `!hasUsableBackup` → 展示**非阻断**告警「没有备用目标，降级将不可用」，**不**阻止保存。
- [ ] 确认既有「启用路由须至少一个启用目标」的硬校验（P2）保持不变。
- [ ] `pnpm test`、`pnpm build`。

### Task 8: 端到端验收与文档同步

**Files:**
- Modify: `docs/后端接口文档.md`
- Modify: `docs/后续功能计划备忘.md`（如实施中决议有变）

- [ ] 端到端验收：配置两目标（A 先 B 后），令 A 稳定 500、B 正常；发一次请求 → 客户端成功、日志两条、金额与手工核对一致；令 A 返回 400 → 不切换。
- [ ] `docs/后端接口文档.md` 同步：`RequestLog.attemptIndex`；降级触发与 429 分类；冷却语义（内存态、带过期）；候选耗尽行为；「首字节后不降级」。
- [ ] 全量 `cargo test`、`cargo clippy -- -D warnings`、`pnpm test`、`pnpm build` 全绿。
