# Config Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把配置 JSON 从「启动时偷偷播种」改造为显式的**跨设备迁移**能力——设置页提供与「导出配置」对称的「导入配置」，用户选文件、看摘要、确认合并。

**Architecture:** 复用现有 `SeedFile` JSON 结构并扩展虚拟密钥段；把仅供首次播种的 `import()` 重写为可反复调用的 `merge_seed()`（合并覆盖 + 摘要计数）。导入 / 导出命令接收前端文件对话框给出的路径，后端只负责读写与事务。彻底移除启动播种路径。

**Tech Stack:** Rust（rusqlite 事务 / serde）、Tauri 2 + `tauri-plugin-dialog`、React 19、TypeScript

**Spec:** `docs/后端接口文档.md`、`docs/UIUX设计文档`

**依赖:** 无。

## 设计决议（已拍板）

- JSON **仅作跨设备迁移载体**，不再用于启动播种。
- **导出范围**：providers（含 `api_key` 明文）、upstream_models、routes + route_targets、virtual_keys（含明文 `key`）。
- **不含** settings（端口是设备相关）、日志、用量。
- **导入语义 = 合并覆盖**：自然键命中则更新字段，未命中则新增；库中未被导入提及的条目**保留**（不删除）。
  自然键：provider = `name`；upstream_model = `(provider name, model_id)`；route = `alias`；virtual_key = `key`。
- route_targets 随所属 route **重建**（先删该 route 的 targets，再按导入插入）。
- 导入前展示**摘要确认**，导入后展示**结果摘要**。
- **移除** `seed_if_empty`、`seed_path_candidates` 与 `lib.rs` 的启动播种块。
- 明文密钥只落文件、不进日志、不在 UI 明文回显导入内容。

## Global Constraints

- 导入必须在**单事务**内完成，任一步失败整体回滚。
- 沿用现有校验：route 协议必须已知且与目标上游模型协议一致；引用不存在的 provider / model 即报错。
- 前端不解析 JSON；读文件、写文件都在后端，前端只传路径。
- 路径来自 `tauri-plugin-dialog`，后端不得自行推断输出目录。
- 不新增图表 / 颜色；设置页样式沿用既有 token（`AGENTS.md`）。
- 接口契约变更后同步 `docs/后端接口文档.md`。

## 非目标（明确不做）

- 不迁移设置、日志、用量与虚拟密钥的已用花费。
- 不做"导入预览全量 diff"页面；只给数量摘要。
- 不做加密 / 口令保护的导出。

---

### Task 1: seed 模块扩展为可复用的 merge

**Files:**
- Modify: `src-tauri/src/db/seed.rs`
- Test: `src-tauri/src/db/seed.rs`（`#[cfg(test)]` 内联）

- [ ] `SeedFile` 增加 `#[serde(default)] virtual_keys: Vec<SeedVirtualKey>`；新增结构 `SeedVirtualKey { key, name, enabled, quota_limit: Option<f64>, quota_period }`（`camelCase` 序列化）。
- [ ] 新增 `ImportSummary { providers, models, routes, virtual_keys }`，每项含 `created` / `updated` 计数（`camelCase`）。
- [ ] 把 `import()` 重写为 `merge_seed(conn, &SeedFile) -> Result<ImportSummary, AppError>`：
      - provider 按 `name` 查已存在 → UPDATE 或 INSERT；
      - upstream_model 按 `(provider name, model_id)` → UPDATE 或 INSERT；
      - route 按 `alias` → UPDATE（含清空并重建其 targets）或 INSERT；
      - virtual_key 按 `key` → UPDATE（name / enabled / quota）或 INSERT；
      - 全程 `unchecked_transaction`，末尾 commit。
- [ ] `export_seed` 增加 `virtual_keys` 查询，输出 `SeedVirtualKey` 列表。
- [ ] 写测试：全新库导入全部 created；二次导入同内容全部 updated 且条数不变；导入新增一条保留库中旧条目；引用不存在的 provider 报错且回滚；协议不一致报错；虚拟密钥 upsert。
- [ ] 运行 `cargo test` 至通过。

### Task 2: 命令层与启动播种移除

**Files:**
- Modify: `src-tauri/src/commands/settings.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/db/seed.rs`

- [ ] `export_seed_cmd` 改为接收 `path: String`，把 JSON 写到该路径，返回写入的路径。
- [ ] 新增 `import_seed_cmd(state, path: String) -> ImportSummary`：读取文件 → `serde_json` 解析 → `merge_seed`。
- [ ] `lib.rs` 删除 `setup` 中的 seed 播种块（`:71-86`）与相关 `use`。
- [ ] 删除 `seed_if_empty` 与 `seed_path_candidates`（含其测试）；保留 `export_seed` / `merge_seed`。
- [ ] 在 `lib.rs` 的 `invoke_handler` 注册 `import_seed_cmd`。
- [ ] 运行 `cargo test` 与 `cargo clippy --all-targets -- -D warnings` 至通过。

### Task 3: 文件对话框插件

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/capabilities/default.json`
- Modify: `package.json`

- [ ] `Cargo.toml` 加 `tauri-plugin-dialog = "2"`，`lib.rs` 注册 `.plugin(tauri_plugin_dialog::init())`。
- [ ] `capabilities/default.json` 的 `permissions` 追加 `"dialog:default"`。
- [ ] `package.json` 加 `"@tauri-apps/plugin-dialog": "^2"` 并安装。
- [ ] 运行 `cargo build` 与 `pnpm build` 确认插件接入无编译错误。

### Task 4: 前端 service

**Files:**
- Modify: `src/services/settings.ts`

- [ ] 新增 `ImportSummary` 类型（与后端字段一致）。
- [ ] `exportConfig(path: string): Promise<string>` → `invoke("export_seed_cmd", { path })`。
- [ ] `importConfig(path: string): Promise<ImportSummary>` → `invoke("import_seed_cmd", { path })`。
- [ ] 浏览器 mock 分支返回可预测的假摘要，保证 `pnpm dev` 不炸。
- [ ] 运行 `pnpm build` 确认类型无误。

### Task 5: 设置页迁移 UI

**Files:**
- Modify: `src/features/settings/SettingsPage.tsx`
- Modify: `src/styles/features/` 内既有设置页样式（按需，不新增色值）

- [ ] 数据区新增「配置迁移」行：**导出配置**（`save` 对话框选路径，默认文件名 `lumen.config.json`，提前提示「文件将包含 API Key 与虚拟密钥明文」）。
- [ ] **导入配置**（`open` 对话框选 `.json`）→ 调用后端解析并**先展示摘要**（将新增 / 覆盖的 provider、模型、路由、密钥数量）→ 用户确认后写库。
- [ ] 导入成功后展示结果摘要并触发配置页刷新（或提示重启后生效，按现有模式取其一）。
- [ ] 移除旧的固定 `data_dir/lumen.seed.json` 路径与「打开所在文件夹」展示，改为对话框交互。
- [ ] 补齐 loading / error / 取消（dialog 返回 `null`）态；明文密钥内容不在 UI 回显。
- [ ] 运行 `pnpm test` 与 `pnpm build` 至通过。

### Task 6: 文档与清理

**Files:**
- Modify: `docs/后端接口文档.md`
- Modify: `src-tauri/lumen.seed.example.json`（改注释性文件名或内容说明，作为迁移格式样例）

- [ ] 文档记录 `export_seed_cmd` / `import_seed_cmd` 的参数、返回 `ImportSummary`、合并覆盖语义与自然键、以及「含明文密钥」的警告。
- [ ] 说明启动不再播种；空库首次启动无配置，需手动配置或导入。
- [ ] 运行 `cargo test`、`cargo clippy -- -D warnings`、`pnpm test`、`pnpm build` 至通过。

### Task 7: 端到端验证

**Files:** 无新增

- [ ] 导出到你选的位置，确认文件含 providers / upstream_models / routes / virtual_keys 四段。
- [ ] 改动若干配置后再次导入同一文件，确认被覆盖回文件内容；库中独有条目仍在。
- [ ] 跨"设备"验证：用 `reset_data` 清空后导入该文件，provider / 模型 / 路由 / 密钥全部恢复。
- [ ] 导入非法 JSON、引用缺失 provider 的文件，确认报错清晰且库未被破坏（回滚）。

