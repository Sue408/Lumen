# M1 地基收口 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在进入降级（M2）与跨协议（M3）之前，把四项前置地基一次铺平——①Schema 增量迁移（数据不再随升级清空）；②`route_targets` 稳定 id 的 upsert；③能力词表后端权威化；④dev / release 数据库物理隔离。

**Architecture:** 迁移从「版本不符即 `reset()`」改为 `PRAGMA user_version` 驱动的阶梯，`db/migrations.rs` 独占 schema 与迁移定义，`db/mod.rs` 只管连接与 `with_db`。`route_targets` 以 `(route_id, upstream_model_id)` 为自然键 upsert，并加唯一索引固化该不变式。能力词表在后端以 const 立权威、保存时校验、前端镜像。dev 与 release 通过不同的数据库文件名隔离。

**Tech Stack:** Rust（rusqlite 0.32）、Tauri 2、React 19、TypeScript、Vite

**Spec:** `docs/后续功能计划备忘.md` §5（M1）/ §6 / §7；`docs/后端接口文档.md`

**依赖:** 无。四项互相独立，建议按 Task 顺序提交。

## 设计决议（已拍板）

- 迁移阶梯只做**增量**（`ALTER TABLE ADD COLUMN` / `CREATE TABLE` / `CREATE [UNIQUE] INDEX`），**永不 `DROP` 用户数据**。
- 迁移前 `user_version > SCHEMA_VERSION` 视为「程序过旧」：**拒绝启动并告警**，不清库。
- 不引入无意义的占位版本：Task 2 的「去重 + 唯一索引」是首个真实迁移，同时验证迁移机制。
- `route_targets` 的自然键为 `(route_id, upstream_model_id)`；同一路由内不允许重复上游模型。
- 能力词表**镜像**（后端 const + 前端字面量 + 两侧一致性测试），**不**走 IPC 运行时拉取——词表极稳定，避免为统一引入加载态。软筛选不做。
- dev / release 数据隔离通过文件名区分，不引入环境变量或额外配置。

## Global Constraints

- 纯计算 / 格式化逻辑独立成不依赖框架的模块并配同目录测试。
- 后端数据结构仍只在 `db/models.rs` 定义一处；迁移 SQL 只在 `db/migrations.rs`。
- 遵守 `AGENTS.md`：颜色只来自 `theme.css`，尺寸只来自 `layout.css`，页面选择器挂在根类下。
- 接口 / 行为契约变更后同步 `docs/后端接口文档.md`。
- **不得**再出现任何「版本升级清空 `request_logs` / `settings`」的路径。
- 验证命令（每任务结束都跑）：`cargo test`、`cargo clippy -- -D warnings`（于 `src-tauri/`）；涉及前端时加 `pnpm test`、`pnpm build`。

## 非目标（明确不做）

- 不做真实的「旧 schema → 新 schema」结构迁移（当前无历史版本，只立机制）。
- 不做能力 / 上下文的软筛选与降级（属 M2 / 暂缓）。
- 不做 `route_targets.id` 的对外暴露或日志引用改造（属 M2）。
- 不引入多连接 / 读连接池、不拆表（属 M4，先测量）。
- 不改 demo 的数据生成逻辑；只隔离其落库位置。

---

### Task 1: Schema 增量迁移框架

**Files:**
- Create: `src-tauri/src/db/migrations.rs`
- Modify: `src-tauri/src/db/mod.rs`

- [ ] 新建 `src-tauri/src/db/migrations.rs`，迁入 `SCHEMA`（现 `db/mod.rs:22-128`）与版本常量：
      ```rust
      /// 每次修改 `SCHEMA` 的结构就 +1，并补一条 `MIGRATIONS` 项；纯加索引不算。
      pub const SCHEMA_VERSION: i64 = 7;

      /// 从 `目标版本 - 1` 到 `目标版本` 的增量语句，按目标版本升序。
      /// 只允许增量；首条真实迁移由 Task 2 提供（此处当前为空）。
      const MIGRATIONS: &[(i64, &str)] = &[];
      ```
- [ ] 实现 `pub fn configure(conn: &Connection) -> Result<(), AppError>`：
      - 读 `user_version`。
      - `version > SCHEMA_VERSION` → `Err(AppError::message(format!("数据库版本 {version} 高于本程序支持的 {SCHEMA_VERSION}，请升级 Lumen")))`。
      - `0 < version < SCHEMA_VERSION` → 调用 `apply(conn, version, MIGRATIONS)`。
      - 最后 `conn.execute_batch(SCHEMA)`（幂等：新库建表、旧库补建新增索引）并 `pragma user_version = SCHEMA_VERSION`。
- [ ] 实现 `fn apply(conn, from: i64, migrations: &[(i64, &str)]) -> Result<(), AppError>`：按序执行 `target > from` 的条目，每条包一层 `unchecked_transaction()`，成功后 `user_version = target`；失败即中止且不提升版本。
- [ ] `db/mod.rs`：`pub mod migrations;`；删除 `SCHEMA`、`SCHEMA_VERSION`、`DROP_ALL`、`reset`、`configure`；`open` / `open_in_memory` 改调 `migrations::configure(&conn)`。
- [ ] 确认 `clear_business_data`（显式用户操作）保留在 `db/mod.rs`，不受影响。
- [ ] 写测试（`migrations.rs` 内 `#[cfg(test)]`）：
      - `fresh_db_lands_on_latest_version`：`open_in_memory` 后 `user_version == SCHEMA_VERSION`。
      - `migrates_without_losing_rows`：内存库插入一个 provider → `user_version = SCHEMA_VERSION - 1` → 再次 `configure` → provider 仍在、版本回到最新。
      - `rejects_future_version`：`user_version = SCHEMA_VERSION + 1` → `configure` 返回 `Err`。
      - `apply_runs_incremental_step`：入库后注入 `[(SCHEMA_VERSION + 1, "ALTER TABLE providers ADD COLUMN scratch TEXT NOT NULL DEFAULT ''")]`，从当前版本迁一级，断言新列存在、原有行数不变。
- [ ] 全量 `cargo test` + `cargo clippy -- -D warnings`。

### Task 2: `route_targets` upsert 化（首个真实迁移）

**Files:**
- Modify: `src-tauri/src/db/migrations.rs`
- Modify: `src-tauri/src/db/routes.rs`

- [ ] `migrations.rs`：`SCHEMA_VERSION` 7 → 8；`MIGRATIONS` 增加：
      ```sql
      DELETE FROM route_targets
       WHERE rowid NOT IN (
         SELECT MIN(rowid) FROM route_targets GROUP BY route_id, upstream_model_id
       );
      CREATE UNIQUE INDEX IF NOT EXISTS idx_route_targets_route_model
        ON route_targets(route_id, upstream_model_id);
      ```
- [ ] `SCHEMA` 末尾补同一句 `CREATE UNIQUE INDEX IF NOT EXISTS idx_route_targets_route_model ...`（新库直接带上）。
- [ ] `save_route`（`db/routes.rs:120-133`）替换「全删全插」为 upsert：
      - 事务内先读取现有 `(upstream_model_id -> id)` 映射。
      - 对输入每个 target：命中 → `UPDATE route_targets SET priority, enabled WHERE id = ?`；未命中 → `INSERT`（新 id）。
      - 输入校验：同一请求内出现重复 `upstream_model_id` → `AppError::message("路由目标重复：{model}")`。
      - 映射中剩余项即被移除的目标 → `DELETE FROM route_targets WHERE id = ?`。
- [ ] 写测试（`db/routes.rs`）：
      - `save_route_preserves_target_ids_across_resaves`：保存 → 记录 `targets[].id` → 再保存（改 priority / enabled）→ id 不变。
      - `save_route_removes_dropped_targets`：两目标 → 再保存只留其一 → 被移除者不在库中。
      - `save_route_rejects_duplicate_target_model`：同请求重复模型 → 报错。
      - `target_id_survives_priority_reorder`：交换两个目标的 priority → id 与模型对应关系不变。
- [ ] 全量 `cargo test` + `cargo clippy -- -D warnings`。

### Task 3: 能力词表后端权威化

**Files:**
- Modify: `src-tauri/src/db/models.rs`
- Modify: `src-tauri/src/db/providers.rs`
- Modify: `src/features/providers/providerModel.ts`
- Test: `src-tauri/src/db/models.rs`、`src-tauri/src/db/providers.rs`、`src/features/providers/providerModel.test.ts`

- [ ] `models.rs` 新增权威定义：
      ```rust
      pub const CAPABILITY_VISION: &str = "vision";
      pub const CAPABILITY_TOOLS: &str = "tools";
      pub const CAPABILITY_REASONING: &str = "reasoning";
      pub const MODEL_CAPABILITIES: [&str; 3] =
          [CAPABILITY_VISION, CAPABILITY_TOOLS, CAPABILITY_REASONING];

      pub fn is_known_capability(value: &str) -> bool {
          MODEL_CAPABILITIES.contains(&value)
      }
      ```
- [ ] `save_upstream_model`（`db/providers.rs:109`）在校验期拒绝未知标签：任一 `input.capabilities` 不满足 `is_known_capability` → `AppError::message(format!("未知能力标签：{value}"))`。
- [ ] 前端 `providerModel.ts`：`capabilityOrder` 保持为 `["vision", "tools", "reasoning"]`，注释标注「镜像后端 `MODEL_CAPABILITIES`（`src-tauri/src/db/models.rs`），变更需两侧同步」。
- [ ] 写测试：
      - Rust `models.rs`：`known_capabilities_match_pinned_list` 断言 `MODEL_CAPABILITIES == ["vision", "tools", "reasoning"]`。
      - Rust `providers.rs`：`save_upstream_model_rejects_unknown_capability`（未知 → Err）与 `save_upstream_model_accepts_known_capabilities`（三标签 → Ok）。
      - TS `providerModel.test.ts`：断言 `capabilityOrder` 等于同一字面量集合。
- [ ] `cargo test`、`cargo clippy -- -D warnings`、`pnpm test`、`pnpm build`。

### Task 4: dev / release 数据库隔离

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] 在 `lib.rs` 增加：
      ```rust
      /// dev 与 release 使用不同的数据库文件，避免调试 / 演示数据污染真实账本。
      const fn db_file_name() -> &'static str {
          if cfg!(debug_assertions) {
              "lumen-dev.db"
          } else {
              "lumen.db"
          }
      }
      ```
- [ ] `setup`（`lib.rs:104-107`）改为 `let db_path = data_dir.join(db_file_name());`，并在 dev 构建下 `tracing::info!("开发构建使用独立数据库：{}", db_path.display())`。
- [ ] 确认 `#[cfg(debug_assertions)]` 的 `inject_demo_cmd` 注册（`lib.rs:151-158`）与前端 `import.meta.env.DEV` 门（`SettingsPage.tsx:453`）保持不变——三者共同保证演示数据进不了真实库。
- [ ] 写测试：`debug_build_uses_dev_database`（按 `cfg!(debug_assertions)` 断言 `db_file_name()` 取值）与 `release_build_uses_production_database`（`cfg` 相反分支；release 下测试运行于 `--release` 时生效）。
- [ ] `cargo test` + `cargo clippy -- -D warnings`。

### Task 5: 文档同步

**Files:**
- Modify: `docs/后端接口文档.md`
- Modify: `docs/后续功能计划备忘.md`（如实施中决议有变）

- [ ] 更新「开发期数据策略」相关表述：`user_version` 阶梯迁移，**不再**因版本升级清空 `request_logs` / `settings`；`user_version > SCHEMA_VERSION` 拒绝启动。
- [ ] 说明路由保存语义：目标按 `(routeId, upstreamModelId)` upsert，`id` 稳定；重复上游模型被拒。
- [ ] 说明能力标签取值（`vision` / `tools` / `reasoning`）与保存校验。
- [ ] 说明 dev 构建数据库为 `lumen-dev.db`、release 为 `lumen.db`（`app_data_dir` 下）。
- [ ] 全量 `cargo test`、`cargo clippy -- -D warnings`、`pnpm test`、`pnpm build`。

### Task 6: 端到端验证

**Files:** 无新增

- [ ] 造旧库：把 `app_data_dir/lumen.db` 的 `user_version` 手动改为 6（或用一个旧二进制写入数据），启动新版，确认数据保留、日志无「重建空库」告警、版本升到最新。
- [ ] `pnpm tauri dev` 启动，确认使用 `lumen-dev.db`；注入演示数据后，`lumen.db`（release 库）不受影响。
- [ ] 保存路由并再次保存，确认目标 `id` 不变、移除的目标消失、重复模型报友好错误。
- [ ] 用未知能力标签保存模型 → 报错；前端三个标签保存 → 成功。
- [ ] `cargo test` + `cargo clippy -- -D warnings` + `pnpm test` + `pnpm build` 全绿。
