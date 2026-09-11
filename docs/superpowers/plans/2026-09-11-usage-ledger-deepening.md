# Usage Ledger Deepening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把用量统计页从「周期快照」升级为「一本能切开的账」——图表按维度堆叠承载分户，新增一句话归因与质量指标，让总账能回答「谁花的、为什么、划不划算」。

**Architecture:** 后端在现有 `stats.rs` 聚合上增加 key 维度过滤与堆叠层数据；归因与质量实现为可脱离数据库单测的纯函数。前端把单序列图表改造成堆叠面积 / 堆叠柱，堆叠色带末端直接标注即分户账，不再单独挂列表。

**Tech Stack:** Rust（rusqlite / chrono）、React 19、TypeScript、原生 SVG、CSS

**Spec:** `docs/UIUX设计文档`、`docs/后端接口文档.md`

**依赖:** 先完成 `docs/superpowers/plans/2026-09-11-virtual-keys-module.md`（key 维度过滤与「未归属」桶依赖该模块落地的 `virtual_key_id` 写入与 `listVirtualKeys`）。

## 设计决议（已拍板）

- 保留账头一句话 + 四指标 + 周期切换 / 历史导航。
- 新增「按 key 筛选」入口，与周期筛选平行，含「全部 / 未归属 / 各 key」。
- 图表：日 = 累积堆叠面积（去掉昨天对照曲线）、周 = 堆叠柱、月 = 热力图（不变）。
- 堆叠维度联动：筛选 = 全部 → 按 key；筛选 = 单个 key → 按模型。
- 堆叠分层按**花费**排序，取前 4 + 淡墨「其他」；每层色带末端直接标注 `名字 + ¥金额`，即分户账。
- 归因 = 一句话增量（较上期多花多少、主要来自谁、缓存命中变化贡献多少）。
- 质量 = 一行（缓存命中率 / 失败率 / 推理占输出比）。
- 额度进度与预警**不**进入本页（归密钥页）。

## Global Constraints

- 图表只用赭石 / 靛青 / 苔绿 / 藤黄四种矿物颜料；「其他」与「未归属」用墨阶表达，不占用第五种彩色。
- 去脚手架：无网格线、无轴线、无图例块、无阴影渐变；层级靠直接标注，唯一允许的图表浮层是 Tooltip。
- 朱砂绝不进入图表色板。
- 数字一律 `tabular-nums`；颜色不单独承载信息。
- 新样式放 `src/styles/features/usage/` 现有文件内，或新增后于 `src/App.css` 追加一行 `@import`，选择器挂在 `.usage-page` 下。
- 纯计算 / 格式化逻辑独立成不依赖 React 的模块并配 `.test.ts`。
- 接口契约变更后同步 `docs/后端接口文档.md`。

---

### Task 1: 统计过滤支持 key 维度

**Files:**
- Modify: `src-tauri/src/db/stats.rs`
- Test: `src-tauri/src/db/stats.rs`（`#[cfg(test)]` 内联）

- [ ] 引入过滤枚举 `KeyScope { All, Unassigned, Key(String) }`，由命令层的可选 `virtual_key_id` 映射：`None` → `All`，特殊值 `"__unassigned__"` → `Unassigned`，其余 → `Key(id)`。
- [ ] `fetch_rows` 与 `model_costs` 增加 scope 条件：`All` 不加条件；`Unassigned` 用 `virtual_key_id IS NULL`；`Key` 用 `virtual_key_id = ?`。参数化拼接，禁止字符串插值。
- [ ] 写测试：同一周期内插入两个不同 key + 一条 NULL 的日志，断言 `All` / `Unassigned` / 单 key 三种 scope 的 totals 与 model_costs 正确。
- [ ] 运行 `cargo test` 至通过。

### Task 2: 堆叠层数据

**Files:**
- Modify: `src-tauri/src/db/stats.rs`
- Modify: `src-tauri/src/db/models.rs`（如需共享 DTO）
- Test: `src-tauri/src/db/stats.rs`（`#[cfg(test)]` 内联）

- [ ] 新增 DTO `UsageLayer { name: String, tone: String, values: Vec<f64>, amount: f64 }`（`#[serde(rename_all = "camelCase")]`）；`values` 为**累积花费（元）**，长度与周期桶数一致。
- [ ] 堆叠键由 scope 决定：`All` → 按 `virtual_key_id`（NULL 层名为「未归属」）；`Unassigned` 或 `Key` → 按 `upstream_model_name`（缺失回落 `route_alias`，再回落「未知」）。
- [ ] 按各层本期总花费降序，取前 4 层 + 合并其余为「其他」；`tone` 依次取 `ochre` / `indigo` / `moss` / `yellow`，「其他」与「未归属」统一为 `ink`。
- [ ] 复用 `cumulative_series` 的桶索引逻辑，但按层累积**花费**而非 token。
- [ ] 写测试：断言层数 ≤ 5、排序、前 4 合并逻辑、`ink` 归属、累积末点等于该层本期总花费。
- [ ] 运行 `cargo test` 至通过。

### Task 3: 归因纯函数与查询

**Files:**
- Create: `src-tauri/src/db/attribution.rs`
- Modify: `src-tauri/src/db/mod.rs`
- Modify: `src-tauri/src/db/stats.rs`
- Test: `src-tauri/src/db/attribution.rs`（`#[cfg(test)]` 内联）

- [ ] 定义 `Attribution { delta_cost: f64, top_movers: Vec<Mover { name, delta_cost }>, cache: Option<CacheShift { from_rate, to_rate, delta_cost }> }`（`camelCase`）。
- [ ] 纯函数 `build_attribution(prev: &[(String, f64)], curr: &[(String, f64)], prev_cache: (i64, i64), curr_cache: (i64, i64)) -> Attribution`：
      - `delta_cost` = 本期总花费 − 上期总花费；
      - `top_movers` = 按层花费差排序，取正负各不超过 2 条；
      - `cache` 用命中率变化 × 本期可缓存输入量估算对成本的影响（命中率 = cache_read / (cache_read + cache_creation + input)），任一分母为 0 则返回 `None`。
- [ ] 查询层：在 `stats.rs` 增加按 key scope + 周期的「按层聚合」查询，供 `build_attribution` 消费。
- [ ] 写测试覆盖：整体上涨 / 下降、无同期数据、缓存分母为 0、`top_movers` 截断。
- [ ] `db/mod.rs` 注册 `pub mod attribution;` 并运行测试至通过。

### Task 4: 质量指标

**Files:**
- Modify: `src-tauri/src/db/stats.rs`
- Test: `src-tauri/src/db/stats.rs`（`#[cfg(test)]` 内联）

- [ ] 新增 DTO `Quality { cache_hit_rate: f64, error_rate: f64, reasoning_share: f64 }`（0..1，`camelCase`）。
- [ ] 与花费账分离：花费 / token 仍只统计 `status = 'success'`；质量与失败率需要统计**全部**记录，新增独立查询避免污染既有账目。
- [ ] `cache_hit_rate` 口径与 `gateway/usage.rs` 的 `contains_cache_read` 归一保持一致，写注释说明；分母为 0 时返回 0。
- [ ] `error_rate` = error 条数 / 全部条数；`reasoning_share` = `SUM(reasoning_tokens) / SUM(output_tokens)`。
- [ ] 写测试：构造成功 / 失败 / 含缓存与推理的日志，断言三项比率；分母为 0 时返回 0。
- [ ] 运行 `cargo test` 至通过。

### Task 5: 命令参数与接口契约

**Files:**
- Modify: `src-tauri/src/commands/usage.rs`
- Modify: `src-tauri/src/db/stats.rs`（`UsageOverview` 扩展）
- Modify: `docs/后端接口文档.md`

- [ ] `query_usage_overview_cmd` 增加 `virtual_key_id: Option<String>` 参数并透传。
- [ ] `UsageOverview` 扩展：`layers: Vec<UsageLayer>`、`attribution: Attribution`、`quality: Quality`；保留 `series` 供指标对比，前端日视图不再绘制其 `previousValues`。
- [ ] 在 `build_overview` 中组装 scope、layers、attribution、quality；保持上一版字段不删以免破坏既有前端。
- [ ] 同步 `docs/后端接口文档.md`：`UsageOverview` 新字段、`KeyScope` 约定值 `__unassigned__`、堆叠与归因语义。
- [ ] 运行 `cargo test` 与 `cargo clippy -- -D warnings` 至通过。

### Task 6: 前端 service

**Files:**
- Modify: `src/services/usage.ts`

- [ ] `queryUsageOverview` 增加 `keyScope` 参数（`"all" | "unassigned" | string`），映射到后端 `virtual_key_id`。
- [ ] 扩展 `UsagePeriod` 类型以承载 `layers` / `attribution` / `quality`。
- [ ] 更新浏览器 mock：`usageVisualData` 的分层数据、归因文本因子、质量比率，保证 `pnpm dev` 下堆叠图有真实形状。
- [ ] 运行 `pnpm build` 确认类型无误。

### Task 7: 堆叠图表几何纯函数

**Files:**
- Modify: `src/features/usage/chartGeometry.ts`
- Modify: `src/features/usage/chartGeometry.test.ts`
- Modify: `src/features/usage/usageVisualData.ts`
- Modify: `src/features/usage/usageVisualData.test.ts`

- [ ] 新增 `stackedAreaPaths(layers, width, height, yMax)`：把每层累积花费转换为堆叠面积路径（下边界 + 上边界），返回 `[{ tone, name, amount, path }]`。
- [ ] 新增 `stackedBarSegments(layers, bucket, ...)`：返回某桶内各层的堆叠柱矩形与标注锚点。
- [ ] 新增直接标注布局 `labelAnchors(paths)`：在每层末端（或柱顶）给出 `{ x, y, name, amount }`，含防重叠的最小间距规则。
- [ ] 写测试：零层、单层、层数超限、全部为 0、标注锚点不越界。
- [ ] 运行 `pnpm test` 至通过。

### Task 8: 用量页改造与样式

**Files:**
- Modify: `src/features/usage/UsagePage.tsx`
- Modify: `src/features/usage/UsageTrendChart.tsx`
- Modify: `src/features/usage/PeriodUsageCharts.tsx`
- Create: `src/features/usage/AttributionLine.tsx`
- Create: `src/features/usage/QualityLine.tsx`
- Modify: `src/styles/features/usage/charts.css`、`period-charts.css`、`statistics.css`（按需）

- [ ] 顶部在周期控件旁新增 key 筛选（下拉：全部 / 未归属 / 各 key，数据来自 `listVirtualKeys`），切换后整页按 scope 重新查询。
- [ ] 日视图改为 `stackedAreaPaths` 堆叠面积，移除昨天对照曲线与对应图例逻辑。
- [ ] 周视图改为 `stackedBarSegments` 堆叠柱。
- [ ] 月视图保持热力图，不受堆叠影响。
- [ ] 每层末端直接标注 `名字 + ¥金额`；「其他」「未归属」用墨阶 token。
- [ ] `AttributionLine` / `QualityLine` 渲染一行文字，无同期数据或分母为 0 时给出「暂无同期数据」等降级文案。
- [ ] 颜色全部走 theme token；补齐空 / 加载 / 错误 / 单层 / 超 4 层 / 超长名称截断态与 `prefers-reduced-motion` 降级。
- [ ] 确认既有页面 DOM 结构与类名序列未变。

### Task 9: 端到端验证

**Files:** 无新增

- [ ] `pnpm test` 与 `pnpm build` 通过。
- [ ] `cargo test` 与 `cargo clippy -- -D warnings` 通过（于 `src-tauri/`）。
- [ ] 手动验证：切换日 / 周 / 月与 key 筛选，堆叠维度按预期联动（全部→按 key、单 key→按模型）；标注金额之和等于账头总花费；归因一句话与质量行数值与日志明细一致。
