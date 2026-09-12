# 会话维度与请求头映射 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 交付两个相互咬合的能力——①**会话作为日志的查询维度**（捕获，不实体化）；②**per-target 请求头映射**（含会话归一化），解决上游对请求头的严格校验（如 OpenCode Go 的 `x-opencode-session`）。两者合流：会话解析一次，既用于 Lumen 侧归因，又作为头映射的变量喂给上游。

**Architecture:**
- **会话维度**：`request_logs` 加 `session_id` 列。解析逻辑做成纯函数（`gateway/session.rs`）：按可配置的**候选头名表**取第一个非空值。不建 `sessions` 表、不做生命周期、不做合成——会话只是「按 `(virtual_key_id, session_id)` 聚合出来的视图」。前端用**复合描述**（短 id + 首次出现时间 + 请求数 + 花费 + 模型）代替不存在的「会话名」。
- **请求头映射**：`route_targets` 加 `header_rules` JSON 列。纯逻辑模块（`gateway/headers.rs`）从**下游请求头**构建**上游请求头**：`forward`（白名单 + 通配）→ `rename` → `session`（写入解析出的会话值）→ `set`（常量）→ `remove`（黑名单）；provider 的 `extra_headers` 作为基线先铺，**鉴权头永远最后注入且不可被规则覆盖**；`host`/`content-length`/`connection`/`authorization` 等硬黑名单无条件剥离。UI 用**预设模板 + 空状态引导**降低上手门槛。

**Tech Stack:** Rust（rusqlite 0.32 / axum / reqwest）、Tauri 2、React 19、TypeScript、Vite

**Spec:** `docs/后续功能计划备忘.md` §3（路由调用策略）；`docs/后端接口文档.md`；本文件「设计决议」。

**依赖:** M1（迁移框架 / `route_targets` upsert）、M2（降级链 / `attempt_index`）已完成。

## 事实依据（来自源码 / 官方，2026-09-12 调研）

各 agent 的会话语义都外发，只是字段名不同：

| Agent | 会话相关请求头 | 备注 |
|---|---|---|
| OpenCode（`providerID` 以 `opencode` 开头） | `x-opencode-session`、`x-opencode-project`、`x-opencode-request`、`x-opencode-client`、`User-Agent` | OpenCode 自家托管模型 / Go / Zen |
| OpenCode（其它 provider） | `x-session-affinity`、`X-Session-Id`、`User-Agent` | 把 Lumen 配成第三方 provider 时走这条 |
| Codex CLI | `session_id`；另有 `x-codex-turn-metadata`（内含 `session_id`/`turn_id`） | `codex-rs/codex-api/src/requests/headers.rs` |
| Claude Code | `X-Claude-Code-Session-Id`（v2.1.86 起）、`User-Agent: claude-code/<ver> (cli)` | 本地生成 UUID |

**OpenCode Go 的严格度**：缺 `x-opencode-session` 会**报错**（公开报告 + 官方称其用于 prompt cache 分片）。值应为 `ses_…` 形状；**格式是否严格校验未见确证**，故合成时若需要（后续）给 `ses_` 形状。`User-Agent` 亦可能必需。

## 设计决议（已拍板）

- **会话 = 日志维度，不是实体**：不加 `sessions` 表、不做开始/结束/过期、不做 sub-agent 树。统计与追踪全部由 `request_logs` 按 `session_id` 聚合得出。
- **会话解析 = 捕获，不合成**：只从候选头名表取第一个非空值；客户端没带就留空（`NULL`）。**合成（generate）留到后续**，且需要稳定键设计，本计划不做。
- **会话按虚拟密钥隔离**：分组维度为 `(virtual_key_id, session_id)`，避免跨 key 碰撞。
- **候选头名表**：给一份覆盖上表四家的默认值，存 `settings`，UI 可改——不写死成魔法。
- **不做 parent session**：性能无碍，但属 YAGNI；将来要 sub-agent 树再加列（迁移框架已就绪）。
- **不做手动命名**：用复合描述代替；将来可加轻量 `session_labels`。
- **头映射 per-target**：不兼容的是「上游模型」，不是别名；一条路由的降级目标可能跨 provider，规则各管各的。
- **鉴权最后注入、不可被规则覆盖**；`host`/`content-length`/`connection`/`transfer-encoding`/`authorization`/`x-api-key`/`x-goog-api-key` 为**硬黑名单**，无条件剥离。
- **预设是数据、可编辑**：把「哪家要什么头」的知识做成预设模板，但用户能改，符合「自由度交给用户」的原则。
- **body 变换（Grok 的 `tools[]` 过滤）本计划不做**，另行设计。

## Global Constraints

- 网关层（`gateway/`）不依赖 Tauri；会话解析、头规则求值、校验等纯逻辑独立成可单测模块。
- 数据结构仍在 `db/models.rs` 定义一处；迁移只在 `db/migrations.rs`，**只允许增量**（`ADD COLUMN` / 建索引），不得 `DROP`。
- 会话值与头规则不得影响计费口径；`usage.rs` 纯函数不变。
- 接口 / 行为契约变更后同步 `docs/后端接口文档.md`。
- 验证命令（每任务结束都跑）：`cargo test`、`cargo clippy -- -D warnings`（于 `src-tauri/`）；涉及前端时加 `pnpm test`、`pnpm build`。

## 非目标（明确不做）

- 不做会话实体 / 生命周期 / sub-agent 树 / parent session。
- 不做会话值合成（`fallback=generate`）；本计划 `fallback` 仅支持 `none` / 常量。
- 不做会话手动命名。
- 不做 body 变换（工具过滤 / 字段改写）。
- 不做 response 头 / body 变换。
- 不做客户端自动识别并自动套用预设（可后续做提示）。

---

### Task 1: 记账字段 `request_logs.session_id`

**Files:**
- Modify: `src-tauri/src/db/migrations.rs`
- Modify: `src-tauri/src/db/models.rs`
- Modify: `src-tauri/src/db/logs.rs`
- Modify: `src-tauri/src/gateway/usage.rs`（`LogContext` / `build_log`）
- Modify: `src/services/gateway.ts`（`RequestLog` 类型）
- Modify: `src/services/usage.ts`（mock）

- [ ] `migrations.rs`：`SCHEMA_VERSION` +1（实施时 10 → 11）；`SCHEMA` 的 `request_logs` 加 `session_id TEXT`；`MIGRATIONS` 追加：
      ```sql
      ALTER TABLE request_logs ADD COLUMN session_id TEXT;
      ```
      并在 `SCHEMA` 幂等补建索引（纯加索引不算版本）：
      ```sql
      CREATE INDEX IF NOT EXISTS idx_request_logs_session ON request_logs(session_id);
      ```
- [ ] `models.rs`：`RequestLog` 加 `pub session_id: Option<String>`；`from_row` 读取。
- [ ] `db/logs.rs`：`insert_log` 列清单与绑定补 `session_id`。
- [ ] `gateway/usage.rs`：`LogContext` 加 `session_id: Option<String>`；`build_log` 透传；既有构造点先补 `None`。
- [ ] 前端 `RequestLog` 加 `sessionId: string | null`；mock 补 `sessionId: null`。
- [ ] 测试：扩展 `migrates_from_legacy_version_without_losing_data` 断言 `request_logs` 行保留且 `session_id` 为 `NULL`；新库含该列。
- [ ] `cargo test`、`cargo clippy -- -D warnings`、`pnpm test`、`pnpm build`。

### Task 2: 会话解析纯逻辑 + 候选头名表

**Files:**
- Create: `src-tauri/src/gateway/session.rs`
- Modify: `src-tauri/src/gateway/mod.rs`（`pub mod session;`）
- Modify: `src-tauri/src/db/settings.rs`（默认候选表 + 读写）
- Modify: `src-tauri/src/db/models.rs`（`Settings` 加字段）

- [ ] `session.rs`：
      ```rust
      /// 覆盖 Codex / Claude Code / OpenCode 的默认候选头名（按优先级）。
      pub const DEFAULT_SESSION_HEADERS: &[&str] = &[
          "x-opencode-session",
          "x-session-affinity",
          "x-session-id",
          "x-claude-code-session-id",
          "session_id",
      ];

      /// 按候选顺序取第一个非空请求头值；找不到返回 None。
      pub fn resolve_session(headers: &HeaderMap, candidates: &[String]) -> Option<String>;
      ```
      头名比较大小写不敏感（`HeaderMap` 原生支持）；空串 / 全空白视为未命中。
- [ ] `settings.rs`：`Settings` 增加 `sessionHeaders: Vec<String>`；默认取 `DEFAULT_SESSION_HEADERS`；持久化为 JSON（沿用现有 settings 键值表）。读写容错：解析失败回落默认。
- [ ] 测试：`resolve_session` 命中第一个候选；跳过空值；全缺返回 `None`；大小写不敏感；设置解析失败回落默认。

### Task 3: 捕获接入 `forward`

**Files:**
- Modify: `src-tauri/src/gateway/handlers.rs`
- Test: `src-tauri/src/gateway/handlers.rs`

- [ ] `forward` 开头（鉴权之后即可）解析一次会话值：
      ```rust
      let session = session::resolve_session(&headers, &state.session_headers());
      ```
      `AppState` 暴露 `session_headers()`（启动时从设置读入，随保存设置刷新）。
- [ ] 该值写入**每次 attempt** 的 `LogContext.session_id`（成功与失败尝试都带，便于按会话追踪失败）。
- [ ] 测试：带 `x-claude-code-session-id` 的请求 → 日志 `session_id` 命中；不带 → `NULL`；多候选时取优先级最高者。

### Task 4: 会话查询（后端）

**Files:**
- Modify: `src-tauri/src/db/logs.rs`（`LogFilter` 加 `sessionId`；`list_sessions` 聚合）
- Modify: `src-tauri/src/commands/*`（新增命令）
- Modify: `src-tauri/src/lib.rs`（注册）

- [ ] `LogFilter` 加 `sessionId: Option<String>`，`list_logs` 的 WHERE 支持等值过滤。
- [ ] 新增聚合查询 `list_sessions(conn, filter) -> Vec<SessionSummary>`：按 `(virtual_key_id, session_id)` 分组，`session_id IS NOT NULL`；返回：
      ```rust
      pub struct SessionSummary {
          pub session_id: String,
          pub virtual_key_id: Option<String>,
          pub first_seen: String,   // MIN(occurred_at)
          pub last_seen: String,    // MAX(occurred_at)
          pub requests: i64,
          pub total_tokens: i64,
          pub cost: f64,
          pub models: Vec<String>,  // 去重后的 upstream_model_name
      }
      ```
      按 `last_seen DESC` 排序，`limit` 默认 50。
- [ ] 新命令：`list_sessions_cmd { filter?, limit? }` → `SessionSummary[]`；注册到 `lib.rs`。
- [ ] 测试：两条同会话 + 一条不同会话 → 聚合正确（requests / cost / models 去重）；`session_id` 为 NULL 的不进结果。

### Task 5: 前端会话筛选 + 复合描述

**Files:**
- Modify: `src/services/usage.ts`（`listSessions` + mock）
- Create: `src/features/logs/sessionLabel.ts`（纯函数）
- Test: `src/features/logs/sessionLabel.test.ts`
- Modify: `src/features/logs/`（筛选控件与列表）

- [ ] 纯函数 `formatSessionLabel(summary) -> string`：短 id（前 8 位 + `…`）+ 首次出现（`MM-DD HH:mm`）+ `${requests} 次` + `${cost}`，例如 `ses_ab12… · 09-12 14:03 · 12 次 · $0.34`。配测试（含边界：无 cost、单次）。
- [ ] 日志页加「会话」筛选下拉，选项用 `formatSessionLabel` 渲染；选中后向 `LogFilter.sessionId` 传值。
- [ ] 服务层 `listSessions()` 走 `list_sessions_cmd`，浏览器 mock 返回空。
- [ ] `pnpm test`、`pnpm build`。

### Task 6: `route_targets.header_rules` 迁移

**Files:**
- Modify: `src-tauri/src/db/migrations.rs`
- Modify: `src-tauri/src/db/models.rs`（`RouteTarget` / `RouteTargetInput`）
- Modify: `src-tauri/src/db/routes.rs`（`save_route` / `list_targets`）
- Modify: `src/services/config/routes.ts`（前端类型）

- [ ] `migrations.rs`：`SCHEMA_VERSION` +1（11 → 12）；`SCHEMA` 的 `route_targets` 加 `header_rules TEXT NOT NULL DEFAULT '{}'`；`MIGRATIONS` 追加：
      ```sql
      ALTER TABLE route_targets ADD COLUMN header_rules TEXT NOT NULL DEFAULT '{}';
      ```
- [ ] `models.rs`：`RouteTarget` 加 `header_rules: HeaderRules`；`RouteTargetInput` 加可选 `header_rules`。`HeaderRules` 定义为可序列化结构（见 Task 7），空规则序列化为 `{}`。
- [ ] `routes.rs`：`save_route` 的 upsert 读写 `header_rules`；`list_targets` 反序列化（解析失败回落空规则并告警，不 panic）。
- [ ] 前端 `RouteTarget` / `RouteTargetInput` 补 `headerRules`。
- [ ] 测试：保存带规则的目标 → 读回一致；空规则存为 `{}`；旧库迁移后默认 `{}`。

### Task 7: 头映射纯逻辑

**Files:**
- Create: `src-tauri/src/gateway/headers.rs`
- Modify: `src-tauri/src/gateway/mod.rs`（`pub mod headers;`）

- [ ] 规则结构（`serde`，camelCase 对齐前端）：
      ```rust
      #[derive(Default, Serialize, Deserialize, Clone)]
      #[serde(rename_all = "camelCase", default)]
      pub struct HeaderRules {
          pub forward: Vec<String>,              // 白名单 / 通配，如 "x-*-session"
          pub rename: BTreeMap<String, String>,  // 精确名 → 新名
          pub set: BTreeMap<String, String>,     // 常量
          pub remove: Vec<String>,               // 黑名单 / 通配
          pub session: Option<SessionRule>,
      }
      #[derive(Serialize, Deserialize, Clone)]
      #[serde(rename_all = "camelCase")]
      pub struct SessionRule {
          #[serde(default)] pub sources: Option<Vec<String>>, // None = 用全局候选表
          pub targets: Vec<String>,                            // 要写入的上游头名
          #[serde(default)] pub fallback: SessionFallback,     // none（默认）/ constant
      }
      ```
- [ ] 求值 `pub fn apply_header_rules(client: &HeaderMap, rules: &HeaderRules, session: Option<&str>, session_sources: &[String]) -> HeaderMap`，顺序：
      1. `forward`：按通配从 `client` 拷贝；
      2. `rename`：把已拷贝的精确名改名为目标名；
      3. `session`：取会话值（`rules.session.sources` 或全局 `session_sources` 解析，或复用传入的 `session`），写入 `targets`；无值且 `fallback=constant` 时写常量；
      4. `set`：常量覆盖；
      5. `remove`：按通配删除；
      6. **硬黑名单**（`host`/`content-length`/`connection`/`transfer-encoding`/`authorization`/`x-api-key`/`x-goog-api-key`）无条件删除。
- [ ] 校验 `pub fn validate_header_rules(rules: &HeaderRules) -> Result<(), String>`：通配合法、非空目标名、禁止把硬黑名单写进 `set`/`forward`。
- [ ] 通配匹配纯函数 `fn name_matches(pattern: &str, name: &str) -> bool`（`*` 通配、大小写不敏感）。
- [ ] 测试：forward 命中/未命中；通配；rename 后原名不再存在；session 命中候选并写入多目标；fallback constant；remove；硬黑名单即使被 `set` 也剥离；校验拒绝非法规则。

### Task 8: `send` 接入头规则（鉴权最后注入）

**Files:**
- Modify: `src-tauri/src/gateway/forward.rs`（`send` 签名与实现）
- Modify: `src-tauri/src/gateway/handlers.rs`（调用点传下游头 + 规则 + 会话）
- Test: `src-tauri/src/gateway/forward.rs` / `handlers.rs`

- [ ] `send` 增加参数：`client_headers: &HeaderMap`、`rules: &HeaderRules`、`session: Option<&str>`、`session_sources: &[String]`。实现顺序：
      1. `apply_header_rules(...)` 得到上游头；
      2. 铺 provider `extra_headers`（**仅在目标头不存在时**，不覆盖规则结果）；
      3. 注入鉴权（`bearer` / `x-api-key` / `x-goog-api-key`）——**最后、无条件覆盖**；
      4. Anthropic 默认 `anthropic-version`（仅当上游头中缺失）。
- [ ] 保持 `.json(body)` 的 `content-type` 由 reqwest 设置；不手动加 `host`。
- [ ] 测试（用 mock 上游回显收到的头）：
      - 客户端带 `anthropic-beta` + 目标 `forward:["anthropic-beta"]` → 上游收到；
      - 客户端虚拟密钥 **绝不**出现在上游头里；
      - `session.targets=["x-opencode-session"]` + 客户端带 `x-claude-code-session-id` → 上游收到映射后的值；
      - `set` 覆盖 provider `extra_headers`，但鉴权仍由网关注入。
- [ ] `cargo test`、`cargo clippy -- -D warnings`。

### Task 9: 前端「请求头映射」区 + 预设 + 引导

**Files:**
- Modify: `src/features/routing/RoutingPage.tsx`（目标行折叠区）
- Create: `src/features/routing/headerRulesModel.ts`（纯逻辑：预设、默认、校验）
- Test: `src/features/routing/headerRulesModel.test.ts`
- Modify: `src/styles/features/config.css`

- [ ] 纯逻辑：`HEADER_PRESETS`（数据，非硬编码魔法）——
      - **OpenCode Go 会话头**：`session.targets=["x-opencode-session"]`、`set={"user-agent":"opencode/local"}`；
      - **转发 Anthropic beta**：`forward=["anthropic-beta","anthropic-version"]`；
      - **自定义**（空白）。
      `applyPreset(draft, presetId) -> HeaderRules`。
- [ ] 目标行加默认折叠的「请求头映射」区，折叠时显示一行摘要（`未配置` / `N 条规则`）。
- [ ] 空状态文案：「上游要求特定请求头？把客户端的头映射过去，例如 `x-opencode-session`。」
- [ ] 预设下拉 + 可编辑的规则字段（forward / set / remove / session.targets）；保存前用与后端一致的规则做前端校验，非法即时报错。
- [ ] 保留既有 DOM 结构与类名序列不变（AGENTS.md）；新增选择器全部挂在路由页根类下；颜色只用 token。
- [ ] `pnpm test`、`pnpm build`。

### Task 10: 文档同步与验收

**Files:**
- Modify: `docs/后端接口文档.md`
- Modify: `docs/后续功能计划备忘.md`（如决议有变）

- [ ] `docs/后端接口文档.md` 同步：`RequestLog.sessionId`；`LogFilter.sessionId`；`SessionSummary` 与 `list_sessions_cmd`；`RouteTarget.headerRules` 的规则形状与**求值顺序 / 硬黑名单 / 鉴权最后注入**；会话候选头名表与默认值。
- [ ] 验收：
      - 用 Claude Code / Codex 各发一次请求 → 日志出现会话值；日志页按会话筛选、复合描述可读；
      - 配一个目标带 `session.targets=["x-opencode-session"]` → 用非 OpenCode 客户端请求，上游收到正确的 `x-opencode-session`；虚拟密钥不泄漏；
      - 迁移：旧库升级后 `request_logs` / `route_targets` 数据保留，新列取默认值。
- [ ] 全量 `cargo test`、`cargo clippy -- -D warnings`、`pnpm test`、`pnpm build` 全绿。

---

## 后续（不在本计划）

- **会话值合成**：客户端不带会话头时的稳定键设计（`虚拟密钥 + 首条 user 消息哈希` 或映射表），以及 `fallback=generate` 的 `ses_` 形状。
- **parent session / sub-agent 树**：`request_logs.parent_session_id` + 树形查询。
- **会话手动命名**：轻量 `session_labels` 表。
- **body 变换**：`tools[]` 过滤等（Grok 兼容），以及连带清理历史 `tool_call` / `tool_result`。
- **客户端自动识别提示**：按 `User-Agent` 检测已知 agent，在日志 / 路由页给出「套用某预设」的非阻断提示。
- **保存期预览**：给定样例请求头，预览最终发给上游的头。
