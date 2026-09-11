# Live Usage Refresh Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让用量页、虚拟密钥页、使用日志页在网关持续吃流量时自动更新——开着 GUI 跑 Agent，账本会自己长出来，而不是停在打开那一刻。

**Architecture:** 后端已具备推送能力（每次 `record()` 先写库、再广播 `gateway://log`），本计划**不动后端**。只在前端补一个消费该事件的变化信号 hook，注入三个页面既有的「依赖变化就重拉」声明式 effect。

**Tech Stack:** React 19、TypeScript、Tauri 事件（`@tauri-apps/api/event`）

**Spec:** `docs/后端接口文档.md`、`docs/UIUX设计文档`

**依赖:** 无（`onGatewayLog` 已在 `src/services/gateway.ts` 封装好；任务 2 依赖已完成的两份 plan 的页面结构）。

## 设计决议（已拍板）

- **事件驱动，不轮询**：消费 `gateway://log`，节流合并。
- **节流窗口 1000ms**：窗口内多条请求只触发一次刷新，取窗口内**最后一条** log 作为代表。
- **按需刷新，不是有事件就刷**：
  - 用量页：仅当前周期（`current`）重拉；单 key scope 且代表 log 的 `virtualKeyId` 不匹配时跳过。
  - 密钥页：仅当代表 log 的 `virtualKeyId === 当前选中 key` 时重拉分户。
  - 日志页：无过滤条件，收到信号即重拉。
- **窗口隐藏时挂起，恢复可见时补一次**（避免后台空转）。
- **统一重拉，不做增量插入**：日志 filter 逻辑复杂，重拉更稳；数据量小。

## Global Constraints

- **不改后端**：`gateway://log` 载荷已含 `virtualKeyId` / `cost` / `tokens` / `status` / `occurredAt`。
- **不引入轮询**（`setInterval` 拉取）与全局 context：各页面各自调用 hook，卸载自动清理。
- 实时信号 hook 放 `src/app/`（应用外壳共享）；节流 / 合并的纯逻辑独立成不依赖 React 的模块并配 `.test.ts`。
- 浏览器 mock 环境下 `onGatewayLog` 返回空 unlisten，hook 天然 no-op，不得报错。
- 无新增 CSS；不改既有 DOM 结构与类名序列。
- 硬约束（摘自 `AGENTS.md`）：颜色只来自 `theme.css`，尺寸只来自 `layout.css`。

## 非目标（明确不做）

- 不做「实时刷新」开关（设置项）。若挂机场景实测查询开销高，另立任务。
- 不做日志页增量 prepend。
- 不改 `gateway://status` 的消费（顶部状态灯已订阅）。

---

### Task 1: 实时信号模块与 hook

**Files:**
- Create: `src/app/liveSignal.ts`
- Test: `src/app/liveSignal.test.ts`
- Create: `src/app/useLiveRevision.ts`

- [ ] `liveSignal.ts` 导出常量 `LIVE_REFRESH_WINDOW_MS = 1000`、类型 `LiveSignal { revision: number; lastLog: RequestLog | null }`、纯函数 `mergeLiveSignal(prev: LiveSignal, log: RequestLog): LiveSignal`（`revision + 1`，`lastLog` 取新 log）。
- [ ] 写测试：首次合并 `revision` 为 1、连续合并且 `lastLog` 为最后一条、`lastLog` 不被清空。
- [ ] `useLiveRevision.ts` 导出 `useLiveRevision(): LiveSignal`：
      - 订阅 `onGatewayLog`，把 log 暂存为 pending；
      - 无计时器时启动 `setTimeout(flush, LIVE_REFRESH_WINDOW_MS)`，`flush` 用 `mergeLiveSignal` 更新 state；
      - `document.visibilityState === "hidden"` 时只暂存不排定；`visibilitychange` 恢复可见且仍有 pending 时补一次 flush；
      - 卸载时 `unlisten`、清计时器、移除监听。
- [ ] 运行 `pnpm test` 至通过。

### Task 2: 用量页接入

**Files:**
- Modify: `src/features/usage/UsagePage.tsx`

- [ ] 引入 `useLiveRevision()`，取 `revision` 与 `lastLog`。
- [ ] 在 `overview` 加载 effect 的依赖数组追加 `revision`（现有依赖 `[periodKey, anchorTime, scope]`）。
- [ ] effect 内加**仅刷新时**的短路（`revision > 0` 才生效，避免首挂载被误跳过）：
      - `!current` 直接返回（历史周期不会长出新数据）；
      - scope 为单 key（非 `all`、非 `__unassigned__`）且 `lastLog?.virtualKeyId !== scope` 时返回。
- [ ] 确认 `AnimatedMetricValue` 仅在数值变化时滚动、图表组件未因重拉重播入场动画（堆叠面积 / 堆叠柱当前无入场动画）；若有则抑制。
- [ ] 运行 `pnpm test` 与 `pnpm build` 至通过。

### Task 3: 虚拟密钥页接入

**Files:**
- Modify: `src/features/keys/KeysPage.tsx`

- [ ] 引入 `useLiveRevision()`，取 `revision` 与 `lastLog`。
- [ ] 新增 effect（依赖 `[revision]`）：`revision === 0`、未选中 key（`selectedId` 为 `null` 或 `"new"`）、或 `lastLog?.virtualKeyId !== selectedId` 时返回；否则调用 `loadUsage(selectedId)`。
- [ ] 断言重拉只更新 `usage` state，**不触碰 `draft` / `savedDraft`**，编辑中的草稿不被打断。
- [ ] `keys` 列表不重拉（配额配置不随流量变）。
- [ ] 运行 `pnpm test` 与 `pnpm build` 至通过。

### Task 4: 使用日志页接入

**Files:**
- Modify: `src/features/logs/LogsPage.tsx`

- [ ] 引入 `useLiveRevision()`，取 `revision`。
- [ ] 日志列表加载 effect（依赖 `[filter, limit]`）追加 `revision`。
- [ ] 摘要统计 effect（依赖 `[summaryFilter]`）追加 `revision`。
- [ ] 确认「加载更多」的 `limit` 在重拉后保留、`expanded` 详情态不因重拉丢失。
- [ ] 运行 `pnpm test` 与 `pnpm build` 至通过。

### Task 5: 端到端验证

**Files:** 无新增

- [ ] `pnpm test` 与 `pnpm build` 通过。
- [ ] `cargo test` 与 `cargo clippy -- -D warnings` 通过（于 `src-tauri/`，确认零后端改动未破坏）。
- [ ] 手动验证：网关开启，开着用量页用 Agent 连续打数条请求，账头 / 指标 / 堆叠图在约 1s 内更新；
      切到日志页见新记录置顶且计数增长；切到密钥页，命中当前 key 的请求驱动分户刷新、未命中不刷新；
      单 key scope 下别的 key 的请求不触发用量页重拉；窗口最小化期间不刷新、恢复后补一次。

