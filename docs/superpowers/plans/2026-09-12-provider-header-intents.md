# 请求头四意图重构 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** 把请求头映射从「per-target 通用改写引擎」收敛为「**per-provider 的四意图** + **默认安全底座**」，并补一份网关请求头行为说明书。四意图 = **添加 / 透传 / 替换 / 移除**。

**Architecture:**
- **换轴**：请求头配置从 `route_targets` 迁到 `providers`。请求头是协议/提供商的属性，不是模型的属性；同一 provider 下 N 个模型只配一次。路由/目标层不再有任何请求头配置。
- **四意图**：
  - `添加` = 现有 `Provider.extra_headers`（常量，已落地，保留字段不动）。
  - `透传` = 追加放行客户端头（**任意头名 / 通配，不限于 `x-` 前缀**）。
  - `替换` = 把客户端头 `A` 以 `B` 的名字发出（有序列表，后写覆盖先写）。
  - `移除` = 从最终结果删除（通配，**优先级最高，deny wins**）。
- **默认底座**：内置 `DEFAULT_FORWARD_HEADERS`（`user-agent` 等 Lumen 认为安全可放行的头），开箱即"像直连"；用户无需为常见安全头配置。底座条目可被 `移除` 否决。
- **求值顺序（固定）**：
  `内置底座 → 透传 → 替换 → 添加(extra_headers) → 移除 → 硬黑名单 → 鉴权最后注入 → anthropic-version 兜底`
- **字符集**：头名模式按 RFC 7230 token 放开（含 `_` `.` 等），修掉现在 `session_id` 这类非 `x-` / 带下划线的头名填不进去的 bug。
- **会话**：全局会话候选表（`settings.sessionHeaders`）+ `gateway/session.rs` 仅服务**记账**，保留不变；per-target 的 `session` 规则删除，会话头改写改由 `替换` 表达（Go 不保证长期兼容 Codex/Claude Code 原生头，`替换` 作对冲）。

**Tech Stack:** Rust（rusqlite 0.32 / axum / reqwest）、Tauri 2、React 19、TypeScript、Vite

**Spec:** `docs/superpowers/plans/2026-09-12-session-dimension-and-header-mapping.md`（被本计划取代的头映射部分）；`docs/后端接口文档.md`；`opencode.ai/docs/go`（会话头稳定性）。

**依赖:** 上一计划已完成（`route_targets.header_rules` / `gateway/headers.rs` / `gateway/session.rs` 均已存在）。

## 事实依据（2026-09-12 调研）

- **Go 要求的是稳定，不是格式**：官方原文 "Send a **stable** session ID in `x-opencode-session` for each conversation so we can optimize routing and prompt caching."；`x-opencode-session` 用于**粘性路由**到后端副本，随机值会落到坏副本（`anomalyco/opencode#46011`）。第三方 relay 直接透传 `x-session-affinity` / `x-session-id` 的值、不做 `ses_` 前缀归一化。
- **Go 认识 Codex / Claude Code 原生会话头，但不保证以后兼容**；且"proxy setups still omit it; preserve the session header when forwarding"。故 Lumen 至少要能放行原生头，`替换` 作保险。
- **litellm 的教训**：其默认只放行 `x-*` 结构，导致 Codex 等非 `x-` 前缀的会话头无法放行。本计划禁止这种前缀限定。
- **现状**：`Provider.extra_headers: BTreeMap<String,String>` 已存在（`db/models.rs:50`），即"添加"已落地。
- **现状 bug**：`forward`/`remove` 模式校验只允许 `[A-Za-z0-9-*]`（`gateway/headers.rs:194`、`src/features/routing/headerRulesModel.ts:162`），`_` / `.` 被拒。

## 设计决议（已拍板）

- **配置轴 = provider**：`providers.header_rules`；路由目标层配置全部移除。
- **底座非空**：内置默认放行表，保守起步；不做"默认全发"（会漏 cookie / 私有头）。
- **`session` 特例不再是一等字段**：意图收敛为四类；会话头映射用 `替换`。
- **鉴权最后注入、不可被规则覆盖**；`host` / `content-length` / `connection` / `transfer-encoding` / `authorization` / `x-api-key` / `x-goog-api-key` 为**硬黑名单**，无条件剥离。
- **`route_targets.header_rules` 停用**：保留列不删（避免重建带 FK 的表），代码路径移除，文档标注 deprecated；将来需要清理再走重建表迁移。
- **不做会话合成（generate）**（沿用上一决议）。

## Global Constraints

- 网关层（`gateway/`）不依赖 Tauri；头规则求值与校验是纯函数、可单测。
- 数据结构仍在 `db/models.rs` 一处定义；迁移只在 `db/migrations.rs`，**只允许增量**（`ADD COLUMN` / 建索引 / 重建表套路），不得 `DROP` 用户数据。
- 头规则不得影响计费口径；`usage.rs` 纯函数不变；会话记账（`session.rs` / `settings.sessionHeaders`）不变。
- 前端颜色/尺寸只用 token；新样式以页面根类作用域隔离；不硬编码色值。
- 接口 / 行为契约变更后同步 `docs/后端接口文档.md`。
- 验证命令（每任务结束都跑）：`cargo test`、`cargo clippy -- -D warnings`（于 `src-tauri/`）；涉前端加 `pnpm test`、`pnpm build`。

## 非目标（明确不做）

- 不做"默认全发"底座。
- 不做会话值合成（`fallback=generate`）。
- 不做 body / query / path / response 变换。
- 不做"命名规则集 / 跨 provider 复用"。
- 不做 `route_targets.header_rules` 列的物理删除（仅停用）。

---

### Task 1: 头名模式字符集放开（修复非 `x-` 头名拦截）

**Files:**
- Modify: `src-tauri/src/gateway/headers.rs`（`validate_pattern`）
- Modify: `src/features/routing/headerRulesModel.ts`（`PATTERN_CHARS`）
- Test: `src-tauri/src/gateway/headers.rs`、`src/features/routing/headerRulesModel.test.ts`

- [x] 后端：把头名模式合法字符从 `alnum + '-' + '*'` 放开为 RFC 7230 `tchar`（`!#$%&'*+-.^_`|~` + 字母数字），`*` 继续作为通配；保留"不得命中硬黑名单"的校验。
- [x] 前端 `PATTERN_CHARS` 同步为同一集合。
- [x] 测试：
      - 后端：`validate_header_rules` 接受 `session_id`、`x-codex-turn-metadata`、`anthropic-beta`；接受 `x-*`；仍拒绝空白与命中黑名单的模式。
      - 前端：`validateHeaderRules` 接受 `session_id`；`nameMatches("session_id","session_id")` 为真。
- [x] `cargo test`、`cargo clippy -- -D warnings`、`pnpm test`、`pnpm build`。

### Task 2: `providers.header_rules` 迁移与结构

**Files:**
- Modify: `src-tauri/src/db/migrations.rs`
- Modify: `src-tauri/src/db/models.rs`（`Provider` / `ProviderInput`）
- Modify: `src-tauri/src/db/providers.rs`（读写 + 保存期校验）
- Modify: `src/services/config/types.ts`、`src/services/config/mock.ts`、`src/services/config/index.ts`

- [x] `migrations.rs`：`SCHEMA_VERSION` +1（12 → 13）；`SCHEMA` 的 `providers` 加 `header_rules TEXT NOT NULL DEFAULT '{}'`；`MIGRATIONS` 追加：
      ```sql
      ALTER TABLE providers ADD COLUMN header_rules TEXT NOT NULL DEFAULT '{}';
      ```
- [x] `models.rs`：定义
      ```rust
      #[derive(Default, Serialize, Deserialize, Clone)]
      #[serde(rename_all = "camelCase", default)]
      pub struct ProviderHeaderRules {
          pub forward: Vec<String>,                 // 放行（通配）
          pub replace: Vec<HeaderReplace>,          // A → B，有序
          pub remove: Vec<String>,                  // 删除（通配），最高优先级
      }
      #[derive(Serialize, Deserialize, Clone)]
      #[serde(rename_all = "camelCase")]
      pub struct HeaderReplace { pub from: String, pub to: String }
      ```
      `Provider` / `ProviderInput` 加 `header_rules: ProviderHeaderRules`（空序列化为 `{}`）；`from_row` 解析失败回落默认并告警（沿用 `parse_header_rules` 模式，改名为 `parse_provider_header_rules` 或复用）。
- [x] `providers.rs`：upsert 读写 `header_rules`；保存前调用 `gateway::headers::validate_provider_header_rules`（Task 3 提供），非法即拒。
- [x] 前端类型：`Provider` / `ProviderInput` 加 `headerRules: { forward: string[]; replace: {from:string;to:string}[]; remove: string[] }`；`emptyProviderHeaderRules()`；mock 数据补默认空规则。
- [x] 测试：保存带规则 provider → 读回一致；空规则存为 `{}`；旧库迁移后 provider 行保留且 `header_rules` 为 `{}`。
- [x] 四处验证命令。

### Task 3: 头规则求值重构（底座 + 四意图，去 session/rename/set）

**Files:**
- Modify: `src-tauri/src/gateway/headers.rs`
- Modify: `src-tauri/src/db/models.rs`（删除 `HeaderRules` / `SessionRule` / `SessionFallback`，如无他处引用）

- [x] 内置底座（保守起步，注释说明可扩展）：
      ```rust
      pub const DEFAULT_FORWARD_HEADERS: &[&str] = &[
          "user-agent",
          "accept-language",
          "traceparent",
          "tracestate",
          "anthropic-version",
          "anthropic-beta",
          "openai-beta",
          "openai-organization",
          "openai-project",
      ];
      ```
- [x] 求值 `pub fn build_upstream_headers(client: &HeaderMap, rules: &ProviderHeaderRules, extra_headers: &BTreeMap<String,String>) -> HeaderMap`，顺序：
      1. **底座**：`DEFAULT_FORWARD_HEADERS` 逐个从 `client` 拷贝（存在才拷）；
      2. **透传**：`rules.forward` 按通配从 `client` 拷贝（多值保留）；
      3. **替换**：按序把 `client[from]` 的值写到 `to`（`from` 是否存在不影响其在底座/透传中的去留）；
      4. **添加**：`extra_headers` 常量 `insert`（覆盖）；
      5. **移除**：`rules.remove` 按通配删除（在最终结果上，deny wins）；
      6. **硬黑名单**：无条件剥离。
- [x] 校验 `pub fn validate_provider_header_rules(rules) -> Result<(), String>`：
      - `forward` / `remove`：非空、token 合法、不得命中硬黑名单；
      - `replace.from` / `replace.to`：非空、token 合法、`from` 不得命中硬黑名单（`to` 命中黑名单将被静默剥离，但保存时也拒绝以早暴露）。
- [x] 删除不再使用的 `HeaderRules` / `SessionRule` / `SessionFallback`（确认 `db/routes.rs`、`db/seed.rs`、`gateway/resolve.rs` 无残留引用后）。
- [x] 测试：底座放行 `user-agent`；`透传` 放行 `session_id`（非 `x-`）与通配；`替换 session_id → x-opencode-session` 写出正确值；`添加` 覆盖透传/底座同名的值；`移除` 能删掉底座/透传/添加产出的头；硬黑名单在 `添加` 下仍被剥离；校验拒绝空名 / 非法字符 / 黑名单。
- [x] 四处验证命令。

### Task 4: 接入 `send`（用 provider 规则，去掉 target 规则）

**Files:**
- Modify: `src-tauri/src/gateway/resolve.rs`（去掉 `ResolvedRoute.header_rules`，改带 `header_rules`/`extra_headers` 的 provider 字段）
- Modify: `src-tauri/src/gateway/forward.rs`（`send` 应用新求值）
- Modify: `src-tauri/src/db/routes.rs`（保存不再处理 `header_rules`）
- Modify: `src-tauri/src/db/seed.rs`（导出/导入去掉 target `header_rules`）
- Modify: `src-tauri/src/gateway/handlers.rs`（调用点；集成测试适配）

- [x] `resolve.rs`：`ResolvedRoute` 移除 `header_rules`，确保携带 provider 的 `header_rules` 与 `extra_headers`（与现有 `extra_headers` 同源）。
- [x] `forward.rs`：`send` 改为 `build_upstream_headers(client_headers, &route.header_rules, &route.extra_headers)`；删除原 `apply_header_rules` 调用点与 `session` / `session_sources` 参数（会话解析仍只在 `handlers.rs` 为记账服务）。
- [x] `routes.rs` / `seed.rs`：移除 `RouteTarget(Input).header_rules` 的读写与校验（列保留不动）。
- [x] 集成测试：用回显上游断言——底座 `user-agent` 到达；`透传 session_id` 到达；`替换 session_id → x-opencode-session` 到达且下游虚拟密钥不泄漏；`添加` 覆盖；`移除` 生效。
- [x] 四处验证命令。

### Task 5: Provider 表单四意图 UI + 说明书，移除路由页配置

**Files:**
- Create: `src/features/providers/providerHeaderModel.ts`（纯逻辑 + 测试）
- Modify: `src/features/providers/ProviderForm.tsx`
- Modify: `src/features/routing/RoutingPage.tsx`（移除 `HeaderRulesPanel`）
- Delete: `src/features/routing/headerRulesModel.ts` + `.test.ts`（逻辑迁往 provider 侧）
- Modify: `src/styles/features/providers/providers.css`（新样式，token-only）

- [x] `providerHeaderModel.ts`：
      - `DEFAULT_FORWARD_HEADERS`（与后端同表，注释标注镜像）；
      - 文本 ↔ 结构的互转（`textToLines` / `linesToText` / `textToReplaces` / `replacesToText`）；
      - `validateProviderHeaderRules`（与后端同规则）、`summarizeProviderHeaderRules`、`countProviderHeaderRules`；
      - `nameMatches`（与后端同语义）；配测试。
- [x] `ProviderForm.tsx`：新增「请求头映射」区，四个分组：
      - **添加**（复用 `extra_headers` 的 `name: value` 编辑，已有则保留）；
      - **透传**（每行一个，支持 `*`；提示"非 `x-` 前缀的头也能放行，如 `session_id`"）；
      - **替换**（每行 `from → to`）；
      - **移除**（每行一个，支持 `*`）；顶部一段 2~3 行说明书 + 覆盖顺序图示；保存前 `validateProviderHeaderRules`。
- [x] `RoutingPage.tsx`：删除目标行的 `HeaderRulesPanel`、`headerRulesModel` import 与相关 state/handler；删除 `routing.css` 中 `.target-rules*` 死样式。
- [x] 前端类型收尾：确认无 `RouteTarget.headerRules` 残留引用。
- [x] `pnpm test`、`pnpm build`。

### Task 6: 文档与说明书

**Files:**
- Create: `docs/网关请求头行为.md`
- Modify: `docs/后端接口文档.md`

- [x] 新说明书包含：配置轴为何在 provider；四意图定义；默认底座表；固定求值顺序；硬黑名单；鉴权最后注入；"Go 要稳定不要格式"的结论与 `替换` 对冲建议；"代理必须保留/放行会话头"的提醒。
- [x] `后端接口文档.md`：`Provider` / `ProviderInput` 增 `headerRules` 结构；删除 `RouteTarget.headerRules`（标注 deprecated，列暂留）；更新迁移说明（v13）；更新"per-target 请求头映射"段落为 provider 四意图。

### Task 7: 全量验收

- [x] `cargo test`、`cargo clippy -- -D warnings`、`pnpm test`、`pnpm build` 全绿。
- [x] 人工核对：
      - 旧库升级后 providers / request_logs / settings 数据保留，`header_rules` 为 `{}`；
      - Codex 请求：`session_id` 经 `透传` 到达上游；配置 `替换 session_id → x-opencode-session` 后上游收到正确头且值稳定，虚拟密钥不泄漏；
      - 修改 provider 头规则只影响该 provider 下所有模型，路由目标层无重复配置；
      - 说明书与 UI 提示一致。
