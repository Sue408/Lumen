# Usage Statistics Page Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Tauri starter screen with the approved Lumen usage-statistics page and working day/week/month interactions.

**Architecture:** Keep period datasets and SVG chart geometry in focused TypeScript modules, while React components render a stable page structure and swap period-scoped content. Use native SVG and CSS rather than a chart dependency.

**Tech Stack:** React 19, TypeScript 6, Vite 7, CSS, native SVG

**Spec:** `docs/UIUX设计文档` and the approved interactive usage-page prototype

## Global Constraints

- Default page is usage statistics.
- Day, week, and month switches update the whole page.
- The page should fit a normal desktop window without scrolling when space permits.
- Charts use direct labels, mineral colors, no grid scaffolding, gradients, or decorative animation.
- Vermilion is reserved for failure, disabled, and over-budget states.
- Do not add a chart library or connect the Tauri backend in this iteration.

---

### Task 1: Period data and chart geometry

**Files:**
- Create: `src/features/usage/usageData.ts`
- Create: `src/features/usage/chartGeometry.ts`
- Test: `src/features/usage/usageData.test.ts`
- Test: `src/features/usage/chartGeometry.test.ts`

- [ ] Write failing tests for period labels, totals, cost shares, and SVG point generation.
- [ ] Run tests and verify expected failures.
- [ ] Implement the typed mock data and pure geometry helpers.
- [ ] Run tests and verify they pass.

### Task 2: React usage page

**Files:**
- Replace: `src/App.tsx`
- Create: `src/features/usage/UsagePage.tsx`
- Create: `src/features/usage/UsageTrendChart.tsx`
- Create: `src/features/usage/ModelCostBreakdown.tsx`

- [ ] Render the app shell, navigation, summary, metric strip, trend chart, and cost breakdown.
- [ ] Wire the period switcher to all period-scoped content.
- [ ] Add accessible labels and semantic output.

### Task 3: Visual system and responsive verification

**Files:**
- Replace: `src/App.css`
- Modify: `.gitignore`
- Modify: `package.json`

- [ ] Implement paper, ink, typography, spacing, focus, and reduced-motion tokens.
- [ ] Add compact-height and narrow-width behavior with scrolling only below the supported minimum.
- [ ] Run tests and production build.
- [ ] Launch the page, verify day/week/month visually, and compare against the approved prototype.
