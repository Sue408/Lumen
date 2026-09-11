# Virtual Keys Module Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让虚拟密钥成为真正的「分户账 + 额度闸门」——网关强制鉴权、按 key 记账、超限拒绝，并提供完整的密钥管理页面。

**Architecture:** 网关在进入转发前从请求头解析虚拟密钥，查库校验启用状态并计算当前周期累计花费；通过则放行，失败则拒绝并落一条 error 日志。所有日志写入 `virtual_key_id`，用量统计页据此分户。密钥页复用现有 `config-workbench` 主从双栏范式，不发明新语法。

**Tech Stack:** Rust（axum / rusqlite / chrono）、React 19、TypeScript、原生 SVG、CSS

**Spec:** `docs/UIUX设计文档`、`docs/后端接口文档.md`、`docs/后续功能计划备忘.md`

## 设计决议（已拍板）

- **额度计量**：按花费 ¥。
- **强制鉴权**：无 key 一律拒绝；仅 `/v1/chat/completions`、`/v1/messages` 强制；`/health`、`/v1/models` 放行。
- **额度周期**：每个 key 可选 `daily` / `weekly` / `monthly` / `total`（一次性总额）。
- **闸门语义**：建连前检查「已累计花费是否已达上限」，未达即放行——**最后一次请求可能略微超出**；不引入并发锁；流式开流后不撤回。
- **额度预警只在密钥页**展示，总账不背预算。
- **未归属**：历史 `virtual_key_id IS NULL` 的流量保留为「未归属」桶；强制鉴权后新流量不再产生。

## Global Constraints

- 颜色只有一个来源 `src/styles/theme.css`；尺寸只有一个来源 `src/styles/layout.css`。禁止硬编码色值。
- 朱砂只用于超限 / 失败 / 已停用，绝不装饰。额度进度正常用赭石、临近或超限用朱砂，且必配文字。
- 新页面样式放 `src/styles/features/keys/keys.css`，在 `src/App.css` 末尾追加一行 `@import`，选择器挂在 `.keys-page` 根类下。
- 纯逻辑独立成不依赖 React 的模块并配 `.test.ts`；DB 访问统一走 `db::with_db`。
- 修改 `SCHEMA` 必须同步 `SCHEMA_VERSION` +1（启动即重建空库，pre-launch 不做逐列迁移）。
- 新增 IPC 必须带 `isTauriRuntime` 浏览器 mock 回退。
- 接口契约变更后同步 `docs/后端接口文档.md`。

---

### Task 1: 数据模型与 schema 扩展

**Files:**
- Modify: `src-tauri/src/db/mod.rs`
- Modify: `src-tauri/src/db/models.rs`
- Modify: `src-tauri/src/db/keys.rs`

- [ ] 在 `virtual_keys` 建表语句加入 `quota_limit REAL`（NULL = 不限）与 `quota_period TEXT NOT NULL DEFAULT 'monthly'`，`SCHEMA_VERSION` 由 5 改为 6。
- [ ] `VirtualKey` 增加 `quota_limit: Option<f64>`、`quota_period: String`；`from_row` 同步读取。
- [ ] `VirtualKeyInput` 增加 `quota_limit: Option<f64>`、`#[serde(default = "default_quota_period")] quota_period: String`，默认 `monthly`。
- [ ] `save_virtual_key` 的 INSERT / ON CONFLICT UPDATE 写入这两个字段。
- [ ] 运行 `cargo test`（于 `src-tauri/`）确认既有测试仍通过。

### Task 2: 额度周期与判定纯逻辑

**Files:**
- Create: `src-tauri/src/gateway/quota.rs`
- Modify: `src-tauri/src/gateway/mod.rs`
- Test: `src-tauri/src/gateway/quota.rs`（`#[cfg(test)]` 内联）

- [ ] 定义 `QuotaPeriod` 枚举（`Daily` / `Weekly` / `Monthly` / `Total`）与 `parse`（未知值回退 `Monthly`）。
- [ ] 实现 `period_start(period, now: DateTime<Local>) -> DateTime<Local>`：日=当天零点、周=本周一零点、月=月初零点、总额=epoch；与 `db/stats.rs` 的周一起始保持一致。
- [ ] 实现纯判定 `is_over_quota(spent: f64, limit: Option<f64>) -> bool`：`limit` 为 `None` 或 `spent < limit` 返回 false。
- [ ] 写测试覆盖四种周期起点、跨月/跨年边界、`None` 与恰好等于上限的判定。
- [ ] `gateway/mod.rs` 注册 `pub mod quota;` 并运行测试至通过。

### Task 3: 密钥查询与周期花费

**Files:**
- Modify: `src-tauri/src/db/keys.rs`
- Test: `src-tauri/src/db/keys.rs`（`#[cfg(test)]` 内联）

- [ ] 实现 `find_enabled_virtual_key(conn, key: &str) -> Result<Option<VirtualKey>, AppError>`：`WHERE key = ?1 AND enabled = 1`。
- [ ] 实现 `virtual_key_spend(conn, key_id, start: DateTime<Local>) -> Result<f64, AppError>`：
      `SELECT COALESCE(SUM(cost), 0) FROM request_logs WHERE virtual_key_id = ?1 AND status = 'success' AND occurred_at >= ?2`（时间转 UTC RFC3339，与 `stats.rs` 口径一致）。
- [ ] 写测试：插入带 `virtual_key_id` 的成功/失败日志各一条，断言只累计成功且只计入周期内。
- [ ] 运行 `cargo test` 至通过。

### Task 4: 网关鉴权与按 key 记账

**Files:**
- Modify: `src-tauri/src/error.rs`
- Modify: `src-tauri/src/gateway/usage.rs`
- Modify: `src-tauri/src/gateway/handlers.rs`
- Test: `src-tauri/src/gateway/handlers.rs`（`#[cfg(test)]` 内联）

- [ ] `AppError` 增加 `Unauthorized` 与 `QuotaExceeded { name, spent, limit, period }`；`IntoResponse` 分别映射 `401` / `429`，`error_code` 为 `unauthorized` / `quota_exceeded`。
- [ ] `LogContext` 增加 `virtual_key_id: Option<String>`；`build_log` 用它替换硬编码的 `None`。
- [ ] `handlers::forward` 接收 `HeaderMap`，先解析 `Authorization: Bearer <key>`，回退 `x-api-key`。
- [ ] 新增 `authenticate(state, headers) -> Result<VirtualKey, AppError>`：缺失头部 → `Unauthorized`；查库无匹配或停用 → `Unauthorized`。
- [ ] 鉴权发生在 `forward` 内、解析 `model` 之后、`resolve` 之前（此时 alias 可用）；失败时落一条 `virtual_key_id = None`、`http_status = 401` 的 error 日志后返回。
- [ ] 成功与失败的日志均携带已解析的 `virtual_key_id`（鉴权失败的 `reject` 除外）。
- [ ] 测试：带合法 key 的请求日志 `virtual_key_id` 正确；无 key 请求返回 401 且不触发上游；停用 key 返回 401。
- [ ] 运行 `cargo test` 至通过。

### Task 5: 额度闸门接入转发链

**Files:**
- Modify: `src-tauri/src/gateway/handlers.rs`
- Test: `src-tauri/src/gateway/handlers.rs`（`#[cfg(test)]` 内联）

- [ ] 鉴权通过后、`resolve` 之前，用 `quota::period_start(key.quota_period, Local::now())` + `virtual_key_spend` 取已用花费，调用 `quota::is_over_quota`。
- [ ] 超限时返回 `QuotaExceeded`（`429`）并落一条 error 日志（带 `virtual_key_id`），**不调用上游**。
- [ ] 测试：额度上限设为 `0.0` 的 key 一律被拒；上限为 `None` 的 key 正常转发。
- [ ] 运行 `cargo test` 至通过。

### Task 6: 命令与 IPC 契约

**Files:**
- Modify: `src-tauri/src/commands/config.rs`
- Modify: `src-tauri/src/commands/mod.rs`（如需要）
- Modify: `src-tauri/src/lib.rs`
- Modify: `docs/后端接口文档.md`

- [ ] 现有 `list/save/delete_virtual_key_cmd` 随 `VirtualKeyInput` 扩展自动携带额度字段，无需改动签名。
- [ ] 新增 `query_virtual_key_usage_cmd(key_id: String) -> KeyUsageDto`：返回 `{ keyId, spent, calls, limit, period, periodStart }`，其中 `spent` / `calls` 按该 key 自身额度周期统计。`KeyUsageDto` 加 `#[serde(rename_all = "camelCase")]`。
- [ ] 在 `lib.rs` 的 `invoke_handler` 注册新命令。
- [ ] 同步 `docs/后端接口文档.md`：`VirtualKey` / `VirtualKeyInput` 字段、新命令、网关强制鉴权与错误码（`401 unauthorized` / `429 quota_exceeded`）、探测端点放行说明。
- [ ] 运行 `cargo test` 与 `cargo clippy -- -D warnings` 至通过。

### Task 7: 前端 service 扩展

**Files:**
- Modify: `src/services/config.ts`

- [ ] 定义 `VirtualKey` / `VirtualKeyInput` / `KeyUsage` 类型（camelCase，与后端一致）。
- [ ] 实现 `listVirtualKeys` / `saveVirtualKey` / `deleteVirtualKey` / `queryVirtualKeyUsage`，全部带 `isTauriRuntime` mock 回退。
- [ ] mock 数据补 2～3 个 key（含启用/停用、有限额/无限额、超限各一），保证非 Tauri 开发环境可用。
- [ ] 运行 `pnpm build` 确认类型无误。

### Task 8: 密钥页纯逻辑

**Files:**
- Create: `src/features/keys/keyModel.ts`
- Test: `src/features/keys/keyModel.test.ts`

- [ ] 定义 `KeyDraft`（`id` / `name` / `key` / `enabled` / `quotaLimit` / `quotaPeriod`）。
- [ ] `maskKey(key)`：保留 `sk-lumen-` 前缀与首尾各 4 位，中间以 `•` 填充。
- [ ] `emptyKeyDraft()` / `keyToDraft(key)` / `isKeyDraftDirty(draft, saved)` / `validateKeyDraft(draft)`（名称非空、额度为空或 `>= 0` 的十进制）。
- [ ] `quotaPeriodLabel` 映射（每日 / 每周 / 每月 / 一次性总额）与 `quotaSummary(spent, limit, period)`（返回「¥已用 / ¥上限 · 周期」或「¥已用 / 不限」）。
- [ ] `quotaTone(spent, limit)`：未设上限返回 `normal`，`>= limit` 返回 `over`，`>= 0.8 * limit` 返回 `near`。
- [ ] 写测试覆盖掩码、脏检测、校验、摘要与三档 tone 边界。
- [ ] 运行 `pnpm test` 至通过。

### Task 9: 密钥页 React 与样式

**Files:**
- Create: `src/features/keys/KeysPage.tsx`
- Create: `src/styles/features/keys/keys.css`
- Modify: `src/App.css`
- Modify: `src/App.tsx`

- [ ] 按路由页范式搭 `.keys-page > .config-column > .page-header + .config-workbench`，右侧 `workbench-sheet`，复用 `RegisterList` / `SectionTitle` / `TogglePill` / `StatusDot` / `GlyphButton` / `SaveBar` / `InlineError` / `LoadingLines` / `Modal` / `EmptyNote`。
- [ ] 左登记簿：名称 + `StatusDot` + 额度摘要行（超限朱砂、临近赭石、正常淡墨），选中 2px 赭石左规线，行内 `is-off` 表示停用。
- [ ] 右书写台：`sheet-head`（名称 + 状态胶囊 + 删除）；基础信息区（名称、掩码密钥 + 复制 + 轮换）；额度区（计量固定显示「按花费」，上限输入，「不限」开关，周期选择）；分户摘要区（本期花费 / 次数 / 额度进度条）；`SaveBar` 吸底。
- [ ] 新增走 `Modal`；复制用 `navigator.clipboard` 并在成功/失败给出文字反馈；轮换即时把 key 重写为 `sk-lumen-<uuid>` 且旧值失效。
- [ ] `App.tsx` 增加 `case "keys": return <KeysPage />`。
- [ ] `App.css` 末尾追加一行 `@import "./styles/features/keys/keys.css";`。
- [ ] 只用 theme token 上色，进度条临近/超限用朱砂并配文字；补齐 hover / focus-visible / disabled / loading / 空 / 错误态与 `prefers-reduced-motion` 降级。
- [ ] 确认既有页面 DOM 结构与类名序列未变。

### Task 10: 端到端验证

**Files:** 无新增

- [ ] `pnpm test` 与 `pnpm build` 通过。
- [ ] `cargo test` 与 `cargo clippy -- -D warnings` 通过（于 `src-tauri/`）。
- [ ] 手动验证：启动网关 → 无 key 请求 401 → 建 key 后带 key 正常转发 → 日志写入该 key → 额度设为 `0` 后返回 429 → 密钥页摘要与进度条状态正确。
