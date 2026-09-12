# 网关出站统一走本地代理 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 给网关新增一个**可选的出站代理**设置，让所有上游请求统一经由一个本地 HTTP 代理发出；留空即直连。保存后**即时生效**，无需重启或停止网关。

**Architecture:**
- **配置落点**：复用现有 `settings(key, value)` 键值表，新增 `proxy_url` 键——**不改表结构、不加迁移、不动 `SCHEMA_VERSION`**。
- **客户端可换**：`AppState.http` 从不可变的 `reqwest::Client` 改为 `RwLock<reqwest::Client>`；`reqwest::Client` 内部是 Arc，读锁 clone 极廉价。出站只有**一个咽喉点**（`forward::send`），换 client 即全局生效，在途请求继续用旧 client 直至结束。
- **建库统一入口**：把 connect 超时与代理注入收拢到纯函数 `build_http_client(proxy_url) -> Result<Client, AppError>`；启动与保存都走它，非法 URL 在**保存时**即拒。
- **协议范围**：仅 HTTP/HTTPS 代理（`Proxy::all("http://127.0.0.1:7890")`），HTTPS 目标经 `CONNECT` 隧道。**不引入 SOCKS，`Cargo.toml` 不变。**

**Tech Stack:** Rust（rusqlite 0.32 / axum / reqwest 0.12）、Tauri 2、React 19、TypeScript、Vite

**Spec:** `docs/后端接口文档.md`（§1 `Settings`、§2 `save_settings_cmd`）；本文件「设计决议」。

**依赖:** 无需前置。纯增量功能，独立于 M1/M2 与头映射系列。

## 事实依据（来自源码，2026-09-12）

- **单一出站咽喉**：全局只有一个 client，在 `lib.rs:123` 构建，存于 `state.rs:99` 的 `pub http`；生产代码中**唯一**使用点是 `gateway/forward.rs:63` 的 `state.http.post(...)`，由 `handlers.rs:336` 的 `forward::send` 调用（failover 的所有尝试都经此处）。
- **设置是 KV，不是列**：`settings` 表只有 `(key, value)`（`migrations.rs:69`），读写走 `db/settings.rs` 的 `read_value` / `write_value`。**新增一个键不需要迁移**，`settings` 数据也不随升级丢失（`clear_business_data` 明确保留 `settings`）。
- **reqwest 客户端不可变**：代理必须在 `ClientBuilder::build()` 时注入；构建后无法追加。故「运行时切换代理」只能重建并替换 client，或要求重启。
- **代理特性**：reqwest 0.12.28 中 HTTP/HTTPS 代理**无需任何 feature**；仅 SOCKS 需要 `socks` feature（本计划不用）。`Proxy::all(url)` 同时覆盖 http 与 https 目标。
- **错误无法归因到跳数**：`reqwest::Error::url()` 返回的是**目标 URL**，`is_connect()` 也不区分「连代理失败」还是「连上游失败」。故本计划**不做**代理/上游的错误归类（见「非目标」）。

## 设计决议（已拍板）

- **协议只做 HTTP/HTTPS**：URL 形如 `http://127.0.0.1:7890`（Clash / v2ray 的 HTTP 端口）；不支持 SOCKS5，`Cargo.toml` 零新增依赖。
- **保存即时生效**：保存设置时重建 client 并换入 `AppState`；运行中也可切换代理（与端口的「需先停止」语义不同）。
- **留空 = 直连**：`proxy_url` 缺失或空串即不注入代理，行为与现状完全一致。
- **UI 位置**：在 `GatewaySection`（网关分区）内新增一行，与端口、自启、托盘并列；复用现有 `.settings-*` 样式，不新增 CSS 文件。
- **凭据写在 URL 里**：如 `http://user:pass@127.0.0.1:7890`，reqwest 原生支持；与现有 API Key 同为明文存储，UI 给出提示即可，不单独拆用户名/密码字段。
- **模型冷却逻辑完全不动**：代理不可达时沿用现有行为（`handlers.rs:359` 对目标 `mark_cooling` 并降级）。本计划**不**区分代理故障与上游故障，**不**改 `error.rs` / `forward.rs` / `handlers.rs` 的失败分类。

## Global Constraints

- 网关层（`gateway/`）不依赖 Tauri；代理 URL 解析 / client 构建是纯逻辑、可单测。
- 数据结构仍在 `db/models.rs` / `db/settings.rs` 一处定义；本次**无迁移**（`SCHEMA_VERSION` 不变）。
- 代理设置不得影响计费口径；`usage.rs` 纯函数不变；会话与头映射逻辑不变。
- 前端颜色/尺寸只用 token；沿用既有设置页 DOM 类名序列，不新增样式文件即无需改 `App.css`。
- 接口 / 行为契约变更后同步 `docs/后端接口文档.md`。
- 验证命令（每任务结束都跑）：`cargo test`、`cargo clippy -- -D warnings`（于 `src-tauri/`）；涉前端加 `pnpm test`、`pnpm build`。

## 非目标（明确不做）

- 不做 SOCKS4/5 代理。
- 不做代理故障与模型冷却的解耦（用户拍板：冷却不动）。
- 不做 `no_proxy` / 按目标分流的旁路规则。
- 不做代理用户名/密码的独立表单字段。
- 不做「测试代理连通性」按钮 / 保存前主动探测。
- 不改 `clear_business_data` 语义（`settings` 本就保留，代理设置随之保留）。

---

### Task 1: 设置项 `proxy_url`

**Files:**
- Modify: `src-tauri/src/db/settings.rs`

- [x] `Settings` 增字段（camelCase 对齐前端）：
      ```rust
      /// 出站代理 URL（如 http://127.0.0.1:7890）；None 或空串表示直连。
      #[serde(default)]
      pub proxy_url: Option<String>,
      ```
- [x] `get_settings`：读 `proxy_url` 键，缺失 / 空串 / 全空白 → `None`，否则 `Some(trim 后的值)`。
- [x] `save_settings`：`None` 或 trim 后为空 → 写空串（等价关闭）；否则写 trim 后的值。**不在本函数里做 URL 合法性校验**（校验在 Task 4 用 `build_http_client` 做，以便复用 reqwest 的解析语义）。
- [x] `Default` 补 `proxy_url: None`。
- [x] 测试：
      - 默认 `None`；
      - `Some("http://127.0.0.1:7890")` 往返一致；
      - 空串 / 全空白 → 读回 `None`；
      - `None` 保存后再读回 `None`。
- [x] `cargo test`、`cargo clippy -- -D warnings`。

### Task 2: HTTP 客户端可换 + 统一构建函数

**Files:**
- Modify: `src-tauri/src/state.rs`

- [x] 新增纯函数（`AppError` 已在本模块可用）：
      ```rust
      /// 统一构建出站 client：connect 超时固定 10s；传入非空代理 URL 时经该代理。
      pub fn build_http_client(proxy_url: Option<&str>) -> Result<reqwest::Client, AppError> {
          let mut builder = reqwest::Client::builder()
              .connect_timeout(Duration::from_secs(10));
          if let Some(url) = proxy_url.map(str::trim).filter(|url| !url.is_empty()) {
              let proxy = reqwest::Proxy::all(url)
                  .map_err(|error| AppError::message(format!("代理地址无效：{error}")))?;
              builder = builder.proxy(proxy);
          }
          builder.build().map_err(AppError::from)
      }
      ```
- [x] `AppState.http` 字段类型改为 `RwLock<reqwest::Client>`；`AppState::new` 签名**不变**（内部 `RwLock::new(http)`），测试构造点无需改。
- [x] 新增访问器（读锁中毒时从 `into_inner()` 取回，不丢当前 client）：
      ```rust
      pub fn http(&self) -> reqwest::Client {
          self.http.read().map(|guard| guard.clone())
              .unwrap_or_else(|poison| poison.into_inner().clone())
      }
      pub fn set_http(&self, client: reqwest::Client) {
          match self.http.write() {
              Ok(mut guard) => *guard = client,
              Err(poison) => *poison.into_inner() = client,
          }
      }
      ```
- [x] 测试 `build_http_client`：
      - `None` → `Ok`；
      - `Some("")` / `Some("   ")` → `Ok`（视同直连）；
      - `Some("http://127.0.0.1:7890")` → `Ok`；
      - `Some("not a url")`（或其它非法 scheme）→ `Err(AppError::Message(_))`。
- [x] `cargo test`、`cargo clippy -- -D warnings`。

### Task 3: 启动时按设置构建 client

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [x] `setup` 中读取 settings 的元组补上 `proxy_url`：
      ```rust
      let (port, close_to_tray, session_headers, proxy_url) = { /* get_settings */ };
      ```
- [x] 删除内联的 `reqwest::Client::builder()...`，改为：
      ```rust
      let http = crate::state::build_http_client(proxy_url.as_deref())?;
      ```
- [x] `state.set_session_headers(...)` 之后无需额外 setter（client 已由 `AppState::new` 持有）。
- [x] `cargo test`、`cargo clippy -- -D warnings`。

### Task 4: 保存设置时即时换 client

**Files:**
- Modify: `src-tauri/src/commands/settings.rs`

- [x] `save_settings_cmd` 流程：
      1. 保留现有「网关运行中且改变端口 → `AlreadyRunning`」检查（**代理改动不受此限**）；
      2. **先校验并构建**新 client：`let client = build_http_client(input.proxy_url.as_deref())?;`（非法 URL 在此拒绝，尚未落库）；
      3. `with_db(&state.db, ...)` 保存；
      4. 依次 `set_port` / `set_close_to_tray` / `set_session_headers`，并 `state.set_http(client)`。
- [x] 确认 `get_settings_cmd` 无需改动（`Settings` 已含新字段）。
- [x] 测试（参考现有命令测试风格；若纯逻辑难覆盖，至少保证 `build_http_client` 的非法输入被拒——Task 2 已覆盖）：
      - 合法代理保存成功且读回一致；
      - 非法代理保存返回错误、且设置未被写入。
- [x] `cargo test`、`cargo clippy -- -D warnings`。

### Task 5: 出站改用可换 client

**Files:**
- Modify: `src-tauri/src/gateway/forward.rs`

- [x] `send` 中 `state.http.post(url)` → `state.http().post(url)`（`forward.rs:63` 对应行）。
- [x] 确认无其它 `state.http` 直接字段访问残留（全仓 `rg "state\.http\b"`）。
- [x] `cargo test`、`cargo clippy -- -D warnings`。

### Task 6: 前端服务与类型

**Files:**
- Modify: `src/services/settings.ts`

- [x] `Settings` 增 `proxyUrl: string | null`。
- [x] 浏览器 mock 增 `proxyUrl: null`。
- [x] 确认 `getSettings` / `saveSettings` 透传该字段（无需额外逻辑）。
- [x] `pnpm test`、`pnpm build`（此时页面尚未改，编译即验证类型连通）。

### Task 7: 设置页网关分区新增「出站代理」行

**Files:**
- Modify: `src/features/settings/SettingsPage.tsx`
- Modify: `src/features/settings/GatewaySection.tsx`

- [x] `SettingsPage`：
      - 新增 state `proxyUrl: string` 与 `savedProxyUrl: string`；加载时 `setProxyUrl(settings.proxyUrl ?? "")`；
      - 网关保存处理器（原 `submitPort`）改为一次提交端口 + 代理：
        `proxyUrl: proxyUrl.trim() || null`；
      - **关键**：`toggleCloseToTray`、`saveSessionHeaders` 两处的 `saveSettings({...})` payload 也要带上当前 `proxyUrl`（否则会被覆写成 `null`）；
      - 保存成功后的提示区分语义——端口「下次启动生效」，代理「即时生效」。
- [x] `GatewaySection`：
      - 新增 props：`proxyUrl`、`savedProxyUrl`、`onProxyChange`；
      - 新增一行：
        - 标签「出站代理」；
        - `<input class="settings-input" placeholder="http://127.0.0.1:7890">`，`disabled={busy}`（**运行中仍可编辑**）；
        - 说明「留空则直连；支持 http/https 代理，保存后即时生效」；
      - `dirty` 同时纳入代理变更：
        `port.trim() !== String(savedPort) || proxyUrl.trim() !== (savedProxyUrl ?? "")`；
      - `SaveBar` 标签改为「保存网关设置」（一次保存端口 + 代理）。
- [x] 不新增 CSS：复用 `.settings-row` / `.settings-input` / `.settings-row-note`；确认既有 DOM 类名序列与其它行结构一致。
- [x] `pnpm test`、`pnpm build`。

### Task 8: 文档同步与验收

**Files:**
- Modify: `docs/后端接口文档.md`

- [x] §1 `Settings` 类型补：
      ```ts
      proxyUrl: string | null;         // 出站代理 URL，如 http://127.0.0.1:7890；null 表示直连
      ```
- [x] §2 `save_settings_cmd` 说明补：端口改动需网关停止；**代理改动即时生效、无需停止**；非法代理地址会 reject。
- [x] 验收：
      - 留空保存 → 网关行为与现状一致（直连）；
      - 填 `http://127.0.0.1:7890`（本地代理在跑）保存 → 上游请求经代理发出，日志正常记账；
      - 填一个非法字符串保存 → 被拒且原设置不变；
      - 网关运行中切换代理 → 新请求立即走新代理，无需重启；
      - 旧库升级不受影响（无迁移）。
- [x] 全量 `cargo test`、`cargo clippy -- -D warnings`、`pnpm test`、`pnpm build` 全绿。

---

## 后续（不在本计划）

- **SOCKS5 支持**：`reqwest` 开 `socks` feature，`Proxy::all("socks5://...")` 即可；需评估二进制体积。
- **代理故障归因**：保存/请求前探测代理端口，命中则返回独立错误并**跳过模型冷却**——需改 `error.rs` + `forward.rs` + `handlers.rs`（本轮明确不做）。
- **`no_proxy` / 分流**：按目标主机决定是否走代理。
- **测试代理按钮**：设置页主动连通性探测，保存前给即时反馈。
