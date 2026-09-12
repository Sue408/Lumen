# Lumen 项目约定

本文件约束本项目的代码结构与样式规则。核心设计规范见 `docs/UIUX设计文档`，冲突时以设计文档为准。

## 项目背景

**Lumen 是一个运行在本机的个人 LLM 网关**：把多个上游提供商聚合成统一的本地接口，按模型别名把请求路由到对应的上游模型，并记录每一次调用的 token 与花费。产品定位是一本私人的 AI 使用账本——安静、可翻查、只服务一个人，不是控制台也不是仪表盘。

- **技术栈**：Tauri 2（Rust 后端）+ React 19 / TypeScript / Vite（前端），图表用原生 SVG，不引图表库。
- **核心能力**：本地 HTTP 网关（OpenAI 兼容端点）、模型别名路由、用量与花费记账、使用日志流水。
- **设计语言**：见 `docs/UIUX设计文档`（账簿隐喻、纸感、矿物颜料、朱砂铁律、去脚手架图表）。
- **后端原则**：所有数据落 SQLite，配置与流水同库；单价按「每百万 token」存储，费用在网关层计算。

## 前端目录分层

```
src/
  app/            应用外壳：导航注册表、侧边栏、页面框架、路由状态
  components/     无业务的通用 UI 原语（如 WindowChrome、运行时探测）
  services/       跨业务的 IPC 封装：Tauri `invoke`/事件 + 浏览器 mock 回退
  features/<域>/  业务页面与其纯逻辑模块，测试与被测模块同目录
  styles/         全部样式，按 base / shell / features 分区
  App.tsx         组合根：持有顶层视图状态，装配 AppShell 与页面
  App.css         样式聚合入口，只放 @import，不写具体规则
```

规则：

- 新增一个业务页面 = 在 `features/<域>/` 建组件与纯逻辑 + 在 `app/navigation.ts` 注册导航项 + 在 `App.tsx` 挂载。
- 纯计算/格式化逻辑独立成不依赖 React 的模块，并配 `<module>.test.ts`（`node --test` 可跑）。
- 通用、跨业务复用的 UI 才放 `components/`；只有一个域用的组件留在该 `features/<域>/` 内。

## 后端目录分层

```
src-tauri/src/
  lib.rs         组合根：Tauri builder、setup（初始化 DB / 装配状态）、退出清理
  state.rs       共享状态：DB 连接、HTTP 客户端、设置、网关句柄
  error.rs       统一错误类型与 HTTP / IPC 错误映射
  db/            SQLite：连接、migration、各表 CRUD 与查询
  gateway/       本地代理网关：路由解析、上游转发、定价、HTTP handler
  commands/      Tauri 命令：网关启停、配置 CRUD、用量查询
```

规则：

- **数据结构只有一处定义**：所有持久化实体集中放 `db/models.rs`，DTO 与序列化命名（`camelCase`）就地声明。
- **网关与 IPC 解耦**：`gateway/` 只依赖 `state` 与 `db`，不依赖 Tauri；`commands/` 负责把 `gateway` 能力暴露给前端。
- **费用计算独立成纯函数**：放 `gateway/usage.rs`，脱离数据库可直接单测。
- **DB 访问统一走 `db::with_db`**（`spawn_blocking` 包装），不在 handler 里直接持锁跨 `await`。
- 端口默认 `127.0.0.1:8787`，只绑回环；网关默认关闭，由前端显式启停。

## 数据库与迁移

- **改结构 = 同步三处**：`db/migrations.rs` 的 `SCHEMA`（新库建表）、`SCHEMA_VERSION` +1、`MIGRATIONS` 补一条对应目标的增量语句。**只允许增量**（`ADD COLUMN` / 建表 / 建索引），严禁 `DROP` 用户数据；`request_logs` 与 `settings` 不得随升级丢失。
- **删列 / 改类型 / 去约束走重建表**：SQLite 不支持改类型，删列限制也多。需要时用「建新表 → `INSERT ... SELECT` → `DROP` 旧表 → `RENAME` → 重建索引」的重建套路，整段写进一条迁移（`apply` 已包事务）。
- **外键每连接开关**：`configure` 负责 `PRAGMA foreign_keys = ON`；重建带外键的表时，需在事务外先关、迁移后开并 `foreign_key_check`（事务内设置该 pragma 无效）。
- **迁移必配测试**：模拟旧版本库，断言升级后数据保留，参考 `db/migrations.rs` 的 `migrates_without_losing_rows`。
- **dev / release 数据库隔离**：debug 构建用 `lumen-dev.db`，release 用 `lumen.db`（同一 `app_data_dir`）。开发 / 演示数据只进 dev 库，真实账本在 release 库；调试时别指望 dev 能看到 `lumen.db` 的数据。数据库版本号描述的是**表结构**，与 app 版本无关。

## 样式与主题

**颜色只有一个来源：`src/styles/theme.css`。** 所有颜色与阴影都必须写成 token 引用（`var(--ink)`、`var(--chart-ochre)`、`var(--shadow-float)`），**禁止**在页面样式或组件里硬编码十六进制 / `rgb()` 色值。`theme.css` 用 CSS `light-dark()` 按 `color-scheme` 一次声明明暗两套值，`App.css` 将其作为第一行 `@import` 引入。

**尺寸只有一个来源：`src/styles/layout.css`。** 侧边栏宽度、页边距、页面间距等框架尺寸用 `clamp()` 连续插值，**禁止**再写死 `rem` 并靠断点阶跃切换。

- 新增一个颜色语义时，先在 `theme.css` 补 token（明 + 暗两个值），再在调用点引用。
- 非颜色的结构差异（如深色下取消阴影）也通过 token 表达，不要在调用点写主题分支。
- 图表数据只持语义键（如 `ChartTone`），颜色映射由组件层转成 `var(--chart-*)`，数据不出现色值。

## 新页面 CSS 规则

**1. 纯 CSS，不用 CSS Modules / 预处理器 / 原子类。**
现有 DOM 类名是全局语义名且有跨组件协作选择器（如 `.plot-area.is-hovering .hover-guide`），模块化方案会破坏这一约定。

**2. 文件位置与注册。**
新页面样式放 `src/styles/features/<域>/<页面>.css`，并在 `src/App.css` 末尾追加**一行** `@import`。`App.css` 的 `@import` 顺序即层叠顺序，`overrides.css` 之前的历史文件顺序固定不可调换。

**3. 以页面根类作用域隔离。**
每个页面的 `<main>` 使用唯一根类（如 `routing-page`），该页面所有选择器都挂在根类下：`.routing-page .page-header { ... }`。**禁止**在顶层重新定义共享类（`.usage-page`、`.page-header`、`.nav-item` 等），否则会污染现有页面。

**4. 复用 token，不硬编码。**
优先使用 `:root` 中的 `--paper`、`--ink`、`--ink-muted`、`--line`、`--ochre`、`--moss` 等变量；颜色、圆角、边框、字阶一律遵循设计文档。

**5. 不启用 `@layer`（现阶段）。**
遗留 CSS 全部是未分层样式，而未分层的普通声明优先级高于任何分层声明，混用会让新页面样式被历史规则压过。待存量全部迁移后再统一引入 `@layer`，属独立任务。

**6. 状态与无障碍必须齐。**
交互控件需具备 hover / focus-visible / active / disabled / loading；页面需处理空、错误、部分失败、加载、超长截断。任何新增动画都要有 `prefers-reduced-motion` 降级。

**7. 视觉铁律（摘自设计文档）。**
朱砂只用于超预算 / 失败 / 已停用，绝不装饰、绝不进图表；图表只用赭石 / 靛青 / 苔绿 / 藤黄四种矿物颜料；图表去脚手架（无网格线、无轴线、无图例块、无阴影渐变）；数字一律 `tabular-nums`；圆角 2/4/6/pill，描边 1px，focus-visible 用 2px 赭石 outline。

## 验证

- 前端逻辑：`pnpm test`
- 类型与构建：`pnpm build`
- 后端测试：`cargo test`（于 `src-tauri/`）
- 后端静态检查：`cargo clippy -- -D warnings`（于 `src-tauri/`）
- 改动样式或结构后，必须确认既有页面的 DOM 结构与类名序列未变。
