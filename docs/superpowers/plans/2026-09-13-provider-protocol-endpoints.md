# Provider 多协议端点 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让一个上游提供商（同一 base 账号）可以挂载多个**协议端点**（`protocol + baseUrl + authScheme + enabled`），模型仍挂提供商、多协议共享。路由仍是单协议，命中提供商对应端点即**同协议透传**——彻底消除「同一上游支持多协议时要复制整套提供商 + 模型」的重复配置与「忘记改协议」的错配。

**Architecture:** 提供商的连接身份（name / apiKey / 请求头 / 图标）与协议连接细节（协议 / baseUrl / 鉴权）解耦：连接身份留在 `providers`，协议细节下沉到新表 `provider_endpoints`（`UNIQUE(provider_id, protocol)`）。`resolve` 按 `route.protocol` join 端点，`upstream_protocol = route.protocol`（透传），baseUrl / 鉴权取自端点。路由目标合法性校验由「provider.protocol 相等」改为「provider 提供该协议的启用端点」。

**Tech Stack:** Rust（rusqlite 0.32 / axum / reqwest）、Tauri 2、React 19、TypeScript、Vite

**Spec:** `docs/后续功能计划备忘.md`（本计划独立于 M3，见「与 M3 的关系」）；`docs/后端接口文档.md`

**依赖:** M1（迁移框架、`route_targets` upsert）、M2（有序候选、失败分类）均已完成。

## 与 M3 的关系（重要）

本计划是**同协议透传**：上游**原生**就能说 openai / anthropic / responses，网关只是把「一个上游的多个原生端点」登记在一起，零协议转换、零 IR。它与 M3 跨协议转换（`route.protocol` = 下游、`provider.protocol` = 上游、接入 `llmwire` SDK 做转换，见 `docs/后续功能计划备忘.md` §5.3）**正交**，不依赖 SDK，不放开「下游 ≠ 上游」的不一致。

## 设计决议（已拍板）

- **协议端点模型**：`Provider` = 连接身份；`ProviderEndpoint` = 一协议一条端点。端点字段 `protocol / base_url / auth_scheme / enabled`，自然键 `(provider_id, protocol)`，故一个提供商每种协议至多一个端点。
- **baseUrl / authScheme 下沉到端点**：因为真实上游的同一账号不同协议常驻不同 base 与鉴权（例：DeepSeek 的 anthropic 端点是 `https://api.deepseek.com/anthropic/v1` + `x-api-key`，openai 端点是 `/v1` + bearer，见 `db/demo.rs:201-211`）。共享单 base 覆盖不了。
- **模型仍挂提供商、多协议共享**：不重复登记模型；一条 provider 下的模型可同时被不同协议的路由引用（这正是本功能的目的）。
- **路由仍单协议、创建后锁定**：`route.protocol` 是对外契约，不改；每条路由的所有目标都必须有该协议的**启用**端点。
- **移除端点要守卫**：删除某协议端点若被任何路由引用（`r.protocol = that protocol` 且目标属该 provider），保存被拒并指名路由。改动 baseUrl / authScheme **不**受限（只是连接细节）。
- **端点级 `enabled`**：可单独停用某协议，运行时过滤，不会级联影响路由保存校验之外的既有配置。

## Global Constraints

- 网关层（`gateway/`）不依赖 Tauri；数据结构只在 `db/models.rs` 定义一处。
- 迁移只在 `db/migrations.rs`；**只允许增量**（`CREATE TABLE` / `ADD COLUMN`），删列走「重建表」套路且整段进一条迁移。`request_logs` 与 `settings` 不得随升级丢失。
- 每次迁移必配「模拟旧库 → 升级 → 断言行数与金额不丢」的测试。
- 常量在 `src/styles/theme.css` / `src/styles/layout.css`，行为契约变更后同步 `docs/后端接口文档.md`。
- 验证命令（每任务结束都跑）：`cargo test`、`cargo clippy -- -D warnings`（于 `src-tauri/`）；涉及前端时加 `pnpm test`、`pnpm build`。

## 非目标（明确不做）

- 不做任何跨协议转换（属 M3）。
- 不为已有重复 provider 提供自动合并工具。
- 不做「同一模型在不同协议端点下可用性不同」的建模（模型仍提供商级；路由由人手工编排）。
- 不改 `route.protocol` 的语义与锁定规则。

---

### Task 1: 数据模型 `ProviderEndpoint`

**Files:**
- Modify: `src-tauri/src/db/models.rs`

- [ ] 新增 `ProviderEndpoint`（`#[serde(rename_all = "camelCase")]`）：`id / provider_id / protocol / base_url / auth_scheme / enabled`；`from_row`。
- [ ] 新增 `ProviderEndpointInput`：`id: Option<String>`、`protocol`、`base_url`、`#[serde(default = "default_auth_scheme")] auth_scheme`、`#[serde(default = "default_true")] enabled`。
- [ ] 新增 `ProviderWithEndpoints { #[serde(flatten)] provider: Provider, endpoints: Vec<ProviderEndpoint> }`。
- [ ] `Provider` 删除 `base_url / auth_scheme / protocol`；`from_row` 同步删除对应读取（`models.rs:102-119`）。
- [ ] `ProviderInput` 删除同三字段，新增 `endpoints: Vec<ProviderEndpointInput>`。
- [ ] `ProviderEndpoint::from_row` 与 `RouteTarget::from_row` 同款实现。
- [ ] `cargo test`、`cargo clippy -- -D warnings`（本任务会因下游未改而编译失败，允许在 Task 2 一并转绿；若需独立提交，先只加结构、后改引用）。

### Task 2: 迁移 v15（建端点表 + 回填 + 重建 providers）

**Files:**
- Modify: `src-tauri/src/db/migrations.rs`

- [ ] `SCHEMA` 增 `provider_endpoints` 表（幂等 `IF NOT EXISTS`）：
      ```sql
      CREATE TABLE IF NOT EXISTS provider_endpoints (
          id          TEXT PRIMARY KEY,
          provider_id TEXT NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
          protocol    TEXT NOT NULL,
          base_url    TEXT NOT NULL,
          auth_scheme TEXT NOT NULL DEFAULT 'bearer',
          enabled     INTEGER NOT NULL DEFAULT 1,
          UNIQUE(provider_id, protocol)
      );
      ```
- [ ] `SCHEMA_VERSION` 14 → 15；`MIGRATIONS` 追加 v15（单条事务内）：建表 → `INSERT INTO provider_endpoints ... SELECT lower(hex(randomblob(16))), id, protocol, base_url, auth_scheme, 1 FROM providers` → 重建 `providers`（`providers_new` 只留 id/name/api_key/extra_headers/header_rules/icon/icon_tint/enabled/created_at → `INSERT...SELECT` → `DROP TABLE providers` → `ALTER TABLE providers_new RENAME TO providers`）。
- [ ] `configure` 改为迁移期间 FK off、迁移后 on + `PRAGMA foreign_key_check` 校验：因 `providers` 被 `upstream_models` / `provider_endpoints` 外键引用，重建需在事务外先关外键（事务内设置该 pragma 无效）。
- [ ] 重写迁移测试（现有 `insert_provider` / `migrates_without_losing_rows` / `migrates_from_legacy_version_without_losing_data` 直接引用了将消失的列）：
      - 造 v14 旧库：显式 `ALTER TABLE providers ADD COLUMN base_url/auth_scheme/protocol`（带 `DEFAULT`）、`DROP TABLE provider_endpoints`、写入带数据的 provider/route/log/settings、`user_version = 14`。
      - `configure` 后断言：provider 行数不变；每个旧 provider 恰一条端点且 `protocol/base_url/auth_scheme` 一致；route/target/log/金额不丢；`foreign_key_check` 无违规。
      - 保留 `rejects_future_version` / `apply_runs_incremental_step` / `configure_enables_foreign_keys` / 外键级联测试并相应调整。
- [ ] `cargo test`。

### Task 3: 提供商 CRUD（含端点 upsert 与移除守卫）

**Files:**
- Modify: `src-tauri/src/db/providers.rs`

- [ ] `list_providers` 返回 `Vec<ProviderWithEndpoints>`；`get_provider` 返回 `Option<ProviderWithEndpoints>`（端点一次查全，避免 N+1）。
- [ ] 新增 `provider_protocols(conn, provider_id) -> Result<Vec<String>, AppError>`。
- [ ] 新增 `routes_using_protocol(conn, provider_id, protocol) -> Vec<String>`：查引用该 provider 且 `r.protocol = protocol` 的路由别名。
- [ ] `save_provider`：
      1. 校验头规则（沿用 `gateway::headers::validate_provider_header_rules`）。
      2. 校验端点：非空、`is_known_protocol`、协议不重复。
      3. 与旧端点集合 diff，对**被移除的协议**逐个跑 `routes_using_protocol`，有引用即报错并列出别名（替代旧「协议创建后锁定」`providers.rs:52-63`）。
      4. upsert provider 行（不再写 protocol/base_url/auth_scheme）。
      5. 按自然键 `(provider_id, protocol)` upsert 端点、删除已移除的端点（保 id 做法镜像 `route_targets`）。
      6. 返回 `ProviderWithEndpoints`。
- [ ] 测试：保存带多端点；重复协议拒绝；移除被引用协议被拒；移除未被引用协议放行；baseUrl 变更不改端点 id。
- [ ] `cargo test`、`cargo clippy -- -D warnings`。

### Task 4: 路由目标校验改为「端点存在」

**Files:**
- Modify: `src-tauri/src/db/routes.rs`

- [ ] 删除 `target_protocol`（`routes.rs:45-56`），新增 `target_has_endpoint(conn, model_id, route_protocol)`：
      ```sql
      SELECT 1 FROM upstream_models m
      JOIN provider_endpoints e
        ON e.provider_id = m.provider_id AND e.protocol = ?2 AND e.enabled = 1
      WHERE m.id = ?1
      ```
- [ ] `save_route` 目标校验改为「必须有路由协议的启用端点」，错误文案「目标所属提供商未提供 X 协议端点」；**删除「所有目标协议必须等于路由协议」的语义**（混合 openai/anthropic 上游可同挂一条 openai 路由，只要各自都有 openai 端点）。
- [ ] 测试更新：`rejects_mixed_protocol_targets` 改为断言「provider 缺该协议端点才拒绝」；新增「同一 provider 双端点可分别被两条不同协议路由使用」。
- [ ] `cargo test`。

### Task 5: 网关解析与转发

**Files:**
- Modify: `src-tauri/src/gateway/resolve.rs`
- Modify: `src-tauri/src/gateway/handlers.rs`
- Modify: `src-tauri/src/gateway/probe.rs`

- [ ] `resolve_candidates` SQL join 端点：`JOIN provider_endpoints e ON e.provider_id = p.id AND e.protocol = r.protocol AND e.enabled = 1`；`e.base_url AS base_url`、`e.auth_scheme AS auth_scheme`；`upstream_protocol` 取 `r.protocol`（透传恒等）。
- [ ] `handlers::forward` 删除 `first.route_protocol != first.upstream_protocol` 硬校验（`handlers.rs:308-324`）；保留端点协议校验（`handlers.rs:290`）。可选：候选为空时区分「路由无目标」与「目标缺协议端点」。
- [ ] `probe.rs`：`route_for` 入参改为 `(provider, endpoint, model_id)`；`probe` 遍历启用端点逐个探测，返回 `Vec<ProbeResult>`，`ProbeResult` 增 `protocol` 字段。
- [ ] 测试：多协议 provider 对两条路由分别解析出正确 base/auth；停用端点被排除；网关端到端（mock 上游）双协议各 200。
- [ ] `cargo test`、`cargo clippy -- -D warnings`。

### Task 6: 种子导入导出与 demo

**Files:**
- Modify: `src-tauri/src/db/seed.rs`
- Modify: `src-tauri/src/db/demo.rs`

- [ ] `SeedProvider` 增 `endpoints: Vec<SeedEndpoint>`；保留 `base_url / auth_scheme / protocol` 为可选字段做**旧文件兼容**（导入时 `endpoints` 为空且旧字段存在 → 合成一条端点）。
- [ ] 导入：model 的协议集合改由 provider 端点推导；路由目标校验改为「provider 有该协议端点」。
- [ ] 导出：一并导出 `endpoints`。
- [ ] `demo.rs`：`ProviderSpec` 拆分 provider + endpoints，写入端点；可选把 DeepSeek demo 合并为「一个 provider、openai + anthropic 双端点」以演示。
- [ ] 测试：带 endpoints 的 roundtrip；旧格式 seed 导入合成端点；协议不一致目标回滚。
- [ ] `cargo test`。

### Task 7: 前端类型、草稿与逻辑

**Files:**
- Modify: `src/services/config/types.ts`
- Modify: `src/features/providers/providerModel.ts`
- Modify: `src/services/config/mock.ts`

- [ ] `types.ts`：新增 `ProviderEndpoint` / `ProviderEndpointInput`；`Provider` 加 `endpoints`、删 `baseUrl/authScheme/protocol`；`ProviderInput` 同步。
- [ ] `providerModel.ts`：`ProviderDraft` 的 `baseUrl/authScheme/protocol` → `endpoints: EndpointDraft[]`；`emptyProviderDraft` 给默认端点；`providerToDraft` / `isProviderDraftDirty`（端点逐个比较）；`validateProviderDraft` 校验「至少一个端点 + url 合法 + 协议不重复」；新增新端点的协议默认选未占用者、baseUrl/authScheme 预填首行的辅助函数。
- [ ] `mock.ts`：`mockSaveProvider` 写 endpoints；种子数据改为带 endpoints。
- [ ] 测试 `providerModel.test.ts`：端点校验与 dirty 判定。
- [ ] `pnpm test`、`pnpm build`。

### Task 8: 前端表单、页面与路由选择器

**Files:**
- Modify: `src/features/providers/ProviderForm.tsx`
- Modify: `src/features/providers/ProvidersPage.tsx`
- Modify: `src/features/routing/RoutingPage.tsx`
- Modify: `src/services/telemetry.ts`（探测返回数组）
- Modify: `src/styles/features/providers/*`（端点行样式，复用 token）

- [ ] `ProviderForm.tsx`：「协议」单选 → 「协议端点」重复行（协议下拉 + 上游地址 + 鉴权 + 启用 + 删除；「＋ 添加协议端点」）；其余字段不变。
- [ ] `ProvidersPage.tsx`：`submitProvider` / `setProviderAppearance` 传 `endpoints`；摘要行（`ProvidersPage.tsx:509-521`）改为协议 chips + host（host 取端点）；`detectBrand` / `resolveBrand` / `hostLabel` 改用端点 baseUrl；探测 UI 展示多协议结果。
- [ ] `RoutingPage.tsx`：`groupedModels` 过滤条件改为 `provider.endpoints.some(e => e.protocol === protocol && e.enabled)`（建议把 grouping 提成纯函数便于测试）。
- [ ] 新页面样式以 `.providers-page` 作用域隔离，颜色/尺寸只用 token，交互态齐 hover/focus-visible/disabled。
- [ ] `pnpm test`、`pnpm build`；确认既有页面 DOM 结构与类名序列未变。

### Task 9: 文档同步

**Files:**
- Modify: `docs/后端接口文档.md`
- Modify: `docs/后续功能计划备忘.md`
- Modify: `docs/网关请求头行为.md`（确认头映射仍为 provider 级）

- [ ] `后端接口文档.md`：Provider DTO 含 `endpoints`；`test_provider` 返回数组（含 `protocol`）。
- [ ] `后续功能计划备忘.md`：把「provider 单协议锁定」表述改为「多协议端点」，声明独立于 M3。
- [ ] `网关请求头行为.md`：说明头映射仍作用于 provider 级（端点共用）。
- [ ] 全量回归：`cargo test`、`cargo clippy -- -D warnings`、`pnpm test`、`pnpm build`。

---

## 验收场景

1. 建一个 provider，挂 openai + responses + anthropic 三个端点 → 三条路由各选同款模型，三条端点均能 200。
2. 配一个 provider：anthropic 端点 `/anthropic/v1` + `x-api-key`、openai 端点 `/v1` + `bearer`，两条路由各自命中正确 base/auth。
3. 移除被路由引用的协议端点 → 保存被拒并指名路由。
4. 旧库升级：provider / model / route / 流水 / 金额全保留，`foreign_key_check` 干净。
