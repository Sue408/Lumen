# 用量页数据呈现优化（趋势裁左 + 数字分级）Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 解决用量页两个呈现缺陷——(1) 日视图「花费趋势」左端被钉在 00:00，夜间无调用的整点画成一大段贴底空白；(2) 数量显示不做量级自适应，亿级 token 显示为 `10000.0 万`、调用次数显示为 `100,000,000 次`。

**Architecture:** 趋势裁剪是**纯前端**改动——后端日桶仍是当日 24 个整点（`stats.rs` 不动），前端在 `cumulativeToDistribution` 之后、`resampleSeries` 之前把左端无花费的整点裁掉，轴标签与采样标签同步从该点起算，**「今日总账」口径不变**。数字分级在**两端各留一份实现**（后端指标 `value` 已是预格式化字符串，见 `docs/后端接口文档.md:274`），两侧用**同一组临界值测试向量**锁住行为，避免漂移。

**Tech Stack:** Rust（rusqlite / chrono）、React 19 + TypeScript、原生 SVG（不引图表库）。

**Spec:** `docs/后端接口文档.md`（用量总览 DTO：`metrics[].value` 为已格式化串、`series.currentValues` 为累计数值）；`docs/UIUX设计文档`（图表去脚手架、数字 `tabular-nums`）。

## Global Constraints

- **本计划不改 DB schema、不改定价、不改网关转发、不改 llmwire 集成**。
- **同协议口径不变**：日视图仍是「今日 00:00 → 现在」的账，裁剪只是去掉左端无花费前缀，不引入跨零点的滑动窗口。
- 纯计算逻辑独立成不依赖 React 的模块，与被测模块同目录配 `<module>.test.ts`（`node --test` 可跑）。
- 颜色仍只来自 `theme.css`；本计划不新增颜色、不改布局尺寸。
- 验证命令（每任务结束都跑）：
  - 前端：`pnpm test`、`pnpm build`（根目录）
  - 后端：`cargo test`、`cargo clippy -- -D warnings`（于 `src-tauri/`）

## 统一分级规范（唯一契约，两侧共用）

| 区间 | 输出 | 例 |
|---|---|---|
| `0` | `0` | `0` |
| `< 10,000` | 精确整数 + 千分位 | `8,600` |
| `10,000 ~ 1e8` | `{:.1} 万` | `4.8 万` |
| `1e8 ~ 1e12` | `{:.1} 亿` | `1.0 亿` |
| `≥ 1e12` | `{:.1} 万亿` | `1.2 万亿` |

- **进位规则**：先算 `round(v / scale, 1)`，若结果达到下一档整数则提档。必须保证 `99,999,999 → 1.0 亿`（而非 `10000.0 万`）。
- **保留尾零**：`1.0 亿`、`12.0 亿`，与既有 `48.2 万` 风格一致。
- **非有限 / 负值**：按 `0` 处理（沿用现状）。
- **调用次数与 token 同级**，输出形如 `1.2 万次`（空格只加在中西文交界，与 `86 次` 同一规则）。
- **临界值测试向量（两侧必须逐项一致）**：
  `0 / 999 / 8_600 / 9_999 / 10_000 / 48_200 / 99_950_000 / 99_999_999 / 100_000_000 / 120_000_000 / 1_200_000_000 / 999_999_999_999 / 1_000_000_000_000`

## 设计决议（已拍板）

- **日视图保留「今日总账」口径，只裁左端死区**：不改成「最近 N 小时」滑动窗口（那会跨零点，与指标 / 归因 / 同期对比的时间基准冲突）。滚动窗口若日后需要，做成趋势面板内的**独立第三视角**，不在本计划。
- **数字分级走「路径 1：两端各一份实现」**：小改、低风险；`MetricDto.value` 契约不变（仍是预格式化字符串），前端 `metricAnimation` 的老式「反解展示串」本次只补分支，不做结构重构。后续若还要加单位，再整体换成「值 + 单位」结构。
- **金额只补千分位，不做万 / 亿分级**：金额口径是 USD（`docs/后端接口文档.md:9`），`$12,345.67` 比 `$1.2 万` 更合语义；`$` 后不加空格（与 `docs/UIUX设计文档` 排版规则一致）。
- **裁剪只解决左端死区**：`03:00` 自动化与 `10:00` 手动之间的**中间空档保留**——时序数据固有，任何方案都消不掉。

## 非目标（明确不做）

- 不把日视图换成滑动窗口 / 跨零点视角。
- 不改 `MetricDto` / `UsageOverview` 的字段结构。
- 不改 `series.currentValues` 的数值单位（仍是「万 token」的累计数值），只改前端**展示**。
- 不改周柱状图 / 月热力图的框架（固定 7 列 / 整月格子是刻意的账本框架）。
- 不重构 `metricAnimation` 的字符串反解机制。

---

### Task 1: 定契约——分级规范与共用测试向量

**Files:**
- 本文件「统一分级规范」一节即契约；无代码改动。

- [x] 复核上表与临界向量，确认 `99_999_999 → 1.0 亿`、`1_000_000_000_000 → 1.0 万亿` 的期望输出。
- [x] 将向量原样复制到 Task 2（Rust）与 Task 3（TS）的测试中，两侧逐项断言同一结果。

### Task 2: 后端 Rust 数字分级

**Files:**
- Modify: `src-tauri/src/db/stats.rs`

- [x] 新增 `format_compact_count(value: i64) -> String`，实现上表分级与进位规则；取代 `format_tokens_wan`（`stats.rs:492`）。
- [x] 调用次数 metric 改用分级：`format!("{} 次", format_compact_count(current.calls))`（原 `format_thousands`，`stats.rs:645`）。
- [x] 输入 / 输出 token metric 改用 `format_compact_count`（`stats.rs:650`、`stats.rs:655`）。
- [x] 总花费 metric 补千分位、仍保留 2 位小数，且 `$` 后不加空格（`stats.rs`）：`$12,345.67`。
- [x] 清理 `format_thousands`（若无其它调用点则删除，否则保留）。
- [x] 单测：临界向量全表；更新旧断言 `stats.rs:766`（`format_tokens_wan(5_000)` 由 `0.5 万` 改为 `5,000`）与 `stats.rs:794`（`0.5 万` → `5,000`）。
- Verify: `cargo test`、`cargo clippy -- -D warnings`。

### Task 3: 前端 TS 数字分级

**Files:**
- Modify: `src/lib/telemetry.ts`
- Modify: `src/features/usage/metricAnimation.ts`
- Modify: `src/lib/telemetry.test.ts`
- Modify: `src/features/usage/metricAnimation.test.ts`

- [x] `telemetry.ts`：把 `formatCompactTokens`（`telemetry.ts:12`）改造为通用 `formatCompactCount`，实现同一分级与进位规则；修边界 bug（现 `99_999_999` 出 `10000.0 万`）。保留 `formatRate` 不变。
- [x] `metricAnimation.ts`：`formatMetricValue`（`metricAnimation.ts:12`）**补 `亿` / `万亿` 分支**。`parseMetricValue` 无需改（单位因子由 `formatMetricValue` 还原），但需确认 `1.0 亿` 的补间仍渲染为 `x.x 亿`。
- [x] 单测：`telemetry.test.ts` 换用同一临界向量（含 `1_200_000_000 → 12.0 亿`）；`metricAnimation.test.ts` 增 `亿` / `万亿` 用例。
- Verify: `pnpm test`、`pnpm build`。

### Task 4: 其余数量显示调用点统一

**Files:**
- Modify: `src/features/usage/PeriodUsageCharts.tsx`
- Modify: `src/features/logs/LogsPage.tsx`
- Modify: `src/features/usage/UsageTrendChart.tsx`

- [x] `PeriodUsageCharts.tsx:160`、`:168`：热力图 tooltip 去掉硬编码 `万 Tokens`，改用 `formatCompactCount(cell.value * 10_000)` 后接 ` Tokens`（注意 `cell.value` 单位是「万」）。
- [x] `LogsPage.tsx:36` `formatTokens`：改用 `formatCompactCount`，避免亿级长串撑破列表。
- [x] `UsageTrendChart.tsx:36` `formatAxis`（y 轴刻度）：加千分位（金额，不做万 / 亿分级）。
- [x] 单位修正：`UsageTrendChart.tsx:179` 与 `PeriodUsageCharts.tsx:53` 写的是 `元`，但值渲染为 `$`（`UsageTrendChart.tsx:291/322/333`）——按 `docs/后端接口文档.md:9` 的 USD 口径统一改为 `$`。
- Verify: `pnpm test`、`pnpm build`；确认既有页面 DOM 结构与类名序列未变。

### Task 5: 趋势左端裁剪（纯前端）

**Files:**
- Modify: `src/features/usage/trendInteraction.ts`
- Modify: `src/features/usage/trendInteraction.test.ts`
- Modify: `src/features/usage/UsageTrendChart.tsx`

- [x] `trendInteraction.ts` 新增纯函数 `firstActiveBucket(series: number[][]): number`：对各层（分布化后）同下标求和，返回首个 `> 0` 的桶下标；全零返回 `0`。
- [x] 让日视图标签生成支持起点：`getDayLabels(now, count, startMinutes)` 与 `buildPeriodSampleLabels(period, now, count, startBucket)` 从起点起算（保持 `getVisiblePointCount` / `getElapsedBucketCount` 语义不变，新增 `getStartBucket` 组合出可视区间）。
- [x] `UsageTrendChart.tsx:86-106`：在 `cumulativeToDistribution` 之后、`resampleSeries` 之前求 `startBucket`，改为 `slice(startBucket, elapsedBuckets)`；`pointCount = Math.max(elapsedBuckets - startBucket, 2)`；`axisLabels` / `sampleLabels` 同步传 `startBucket`。
- [x] `yMax` / `initialCeiling` / `growCeiling` 逻辑**不动**（峰值不受裁剪影响）。
- [x] 边界：全天无调用时 `startBucket = 0`，保留既有零线表现；`elapsedBuckets - startBucket == 1` 时保持 `Math.max(pointCount - 1, 1)` 的除零保护。
- [x] **对齐校验**：裁剪后 hover tooltip 用的 `sampleLabels[activeIndex]` 必须与 slice 后的点位一一对应，否则时间标签会错位。
- [x] 单测：首桶有值（`startBucket > 0`）/ 中间空档 / 全零 / 仅一个桶四种情形。
- Verify: `pnpm test`、`pnpm build`。

### Task 6: 文档同步与端到端核对

**Files:**
- Modify: `docs/后端接口文档.md`
- Modify: `docs/UIUX设计文档`

- [x] `docs/后端接口文档.md:274`：更新 `value` 示例（如 `"86 次"` / `"1.2 万次"` / `"1.0 亿"` / `"$4.82"`），并就近补一句分级规则。
- [x] `docs/后端接口文档.md:476-477`：说明 `series.currentValues` 仍是「万 token」的**数值**累计（契约不变），前端展示层已分级。
- [x] `docs/UIUX设计文档`：数字规则目前只有 `tabular-nums` 一条，补一行「数量分级」（万 / 亿 / 万亿，1 位小数，保留尾零）。
- [ ] 端到端核对：dev 库（`com.apnea.lumen.dev`）注入 demo 数据，确认指标卡亿级显示为 `1.0 亿` 量级而非 `10000.0 万`；确认日视图曲线左端不再有长空白（`demo.rs:1014` 现断言今日 token > 2000 万，若不足以看到亿级，临时放大样本核对后再还原）。DTO 层已由 `stats.rs::overview_scales_tokens_by_magnitude` 覆盖（断言 `"1.2 亿"`）；GUI 目视待人工执行。
- Verify: `pnpm test`、`pnpm build`、`cargo test`、`cargo clippy -- -D warnings`。

## 风险与注意

- **旧断言会变**：`format_tokens_wan` 的阈值从「0 起就是万」改为「1 万起分级」，`stats.rs:766` / `:794` 两张断言需同步；`telemetry.test.ts:25` 的 `12.0 亿` 保持通过。
- **`metricAnimation` 是反解展示串的老设计**：给后端加新单位时**必须**同步 `formatMetricValue`，否则动画静默丢单位（数字仍会动，只是单位消失）——这是本计划最容易漏的一处。
- **裁剪后标签对齐**：`sampleLabels` 与 slice 必须同源，否则 tooltip 时间错位。
- **`元` / `$` 不一致**属既有笔误，顺手修正；若设计文档另有约定以设计文档为准。
