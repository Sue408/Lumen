# Lumen

运行在本机的个人 LLM 网关与用量账本。把多个上游提供商聚合成统一的本地接口，按模型别名把请求路由到对应的上游模型，并记录每一次调用的 token 与花费。

产品定位是一本私人的 AI 使用账本——安静、可翻查、只服务一个人。

## 特性

- **本地网关**：OpenAI 兼容端点，只绑回环 `127.0.0.1:8787`，默认关闭、显式启停。
- **别名路由**：一条路由对应有序的上游目标，主目标失败时按序降级（同协议）。
- **用量记账**：单价按「每百万 token」存储，费用在网关层计算；失败的上游尝试同样进账，账不低报。
- **使用日志**：按时段 / 别名 / 状态 / 用量可信度筛选与检索。
- **本地优先**：数据全部落本地 SQLite，配置与流水同库，结构变更走增量迁移。

## 技术栈

- 后端：Tauri 2（Rust）、axum、reqwest、rusqlite（bundled）
- 前端：React 19 / TypeScript / Vite；图表为原生 SVG，不引图表库
- 数据库：SQLite，增量迁移见 `src-tauri/src/db/migrations.rs`

## 环境要求

- Node.js + pnpm
- Rust（stable）与 Tauri 2 的前置依赖（见 https://tauri.app/start/prerequisites/ ）

## 开发

```bash
pnpm install
pnpm tauri dev
```

dev 构建使用 `lumen-dev.db`，演示数据不会污染真实账本。

## 构建

```bash
pnpm tauri build
```

release 构建使用 `lumen.db`。

## 测试与检查

```bash
pnpm test                     # 前端逻辑（node --test）
pnpm build                    # 类型检查 + 前端构建
cargo test                    # 后端测试（于 src-tauri/）
cargo clippy -- -D warnings   # 后端静态检查（于 src-tauri/）
```

## 数据位置

数据库位于系统 `app_data_dir`（Windows 为 `%APPDATA%\com.apnea.lumen\`）：

- dev：`lumen-dev.db`
- release：`lumen.db`

## 发布

- **应用标识符**：`com.apnea.lumen`。它决定 `app_data_dir`，发布后不要更改（改则账本换目录）。
- **版本号单一来源**：`package.json` 的 `version`。`src-tauri/tauri.conf.json` 的 `version` 指向 `../package.json`，因此 `getVersion()` 与前端 `__APP_VERSION__` 同源，无需三处手动同步。
- **发布构建**：`pnpm tauri build`，产物在 `src-tauri/target/release/bundle/`。release 构建使用 `lumen.db`。
- **自动更新与代码签名暂缓**：暂未接入更新器（避免在尚无发布渠道时引入额外依赖与体积）；Windows 安装包会触发 SmartScreen 提示。将来需要时见 `docs/后续功能计划备忘.md`。

## 文档

- `AGENTS.md`：项目约定与目录分层
- `docs/UIUX设计文档`：设计语言（账簿隐喻、矿物颜料、图表规范）
- `docs/后端接口文档.md`：网关契约、错误映射与数据库迁移
- `docs/后续功能计划备忘.md`：路线与决策记录
