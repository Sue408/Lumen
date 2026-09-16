<p align="center">
  <img src="assets/brand/lumen-rounded.png" width="96" alt="Lumen logo">
</p>

<h1 align="center">Lumen</h1>

<p align="center"><strong>A local-first LLM gateway and usage ledger for one person.</strong></p>

<p align="center">
  <a href="https://github.com/Sue408/Lumen/releases/latest"><img src="https://img.shields.io/github/v/release/Sue408/Lumen?display_name=tag&sort=semver" alt="Release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-yellow.svg" alt="License: MIT"></a>
  <img src="https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white" alt="Tauri 2">
  <img src="https://img.shields.io/badge/Rust-stable-000000?logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=111111" alt="React 19">
</p>

<p align="center">
  One person's AI gateway, without building an internal platform.<br>
  No Docker. No public endpoint. Multiple models, tracked in one local ledger.
</p>

![Lumen: a local personal LLM gateway and usage ledger](assets/social/github-social-preview.png)

Lumen aggregates multiple upstream providers behind one local endpoint, routes requests to the right upstream through stable model aliases, and records the token usage and cost of every call. It is not a team governance layer, channel distribution system, or commercial relay. It focuses on two things: unified model access and accurate local accounting.

## Core Capabilities

- **Local gateway**: listens only on loopback at `127.0.0.1:8787`, starts disabled, and is enabled explicitly. No server or Docker is required.
- **Alias routing**: clients only need a stable model alias. Targets are ordered by priority, with automatic failover to the next target after a primary failure.
- **Usage accounting**: prices are stored per million tokens, and cost is calculated by the gateway. Cache reads and writes, reasoning tokens, and failed attempts all enter the ledger.
- **Request logs**: inspect calls by period, alias, status, and usage confidence. Errors are attributed to the upstream, the gateway, or the client.
- **Local-first storage**: configuration, API keys, and request history live in a local SQLite database, with incremental migrations on upgrade.
- **Built for one person**: single-user desktop software that stays quiet and reviewable. It does not compete with team platforms such as New API or LiteLLM.

## Download

The first public release is `v0.1.0`. Installers are available from [GitHub Releases](https://github.com/Sue408/Lumen/releases).

- Platform: Windows 10 / 11 x64
- Installer: `Lumen_0.1.0_x64-setup.exe`
- The installer is not code-signed yet, and automatic updates are not available. Windows SmartScreen may warn on first launch.

## Quick Start

1. Start Lumen and add upstream endpoints and API keys under **Providers**.
2. Create a model alias under **Model Routing** and select a primary and optional backup target.
3. Create a local gateway key under **Virtual Keys**.
4. Enable the gateway under **Settings**, then point any OpenAI-compatible client to:

```text
Base URL: http://127.0.0.1:8787/v1
API Key:  sk-lumen-...
Model:    the model alias you created in Lumen
```

5. Make a request and open **Usage** or **Request Logs** to inspect tokens, cost, and the request route.

## Development

Requirements: Node.js, pnpm, Rust stable, and the platform dependencies for Tauri 2. See the [Tauri prerequisites](https://tauri.app/start/prerequisites/).

```bash
pnpm install
pnpm tauri:dev
```

`tauri:dev` uses the `com.apnea.lumen.dev` identifier, so its data directory, single-instance lock, and autostart entry are isolated from an installed release build. The development and release applications can run at the same time.

```bash
pnpm test                     # Frontend logic
pnpm build                    # Type checking and frontend build
cargo test                    # Backend tests (run in src-tauri/)
cargo clippy -- -D warnings   # Backend static checks (run in src-tauri/)
```

## Technology

- Desktop and backend: Tauri 2, Rust, axum, reqwest, rusqlite
- Frontend: React 19, TypeScript, Vite, with charts rendered as native SVG
- Data: SQLite, with configuration and request history in the same database; incremental migrations live in `src-tauri/src/db/migrations.rs`

## Current Boundaries

- Lumen is designed for individual local use. It does not provide multi-user support, organization permissions, channel distribution, or enterprise governance.
- The first public release targets Windows NSIS installers. macOS and Linux are not first-class delivery targets yet.
- It does not claim support for every provider dialect. Unverified protocols and fields are not advertised.
- Automatic updates, code signing, and cloud synchronization are deferred.

## Contributing

- For installation, configuration, or routing issues, open an [Issue](https://github.com/Sue408/Lumen/issues).
- Read [CONTRIBUTING.md](CONTRIBUTING.md) before submitting code.
- Remove API keys, account details, and real billing data from logs, configuration files, and screenshots before submitting them.

## License

[MIT](LICENSE) © 2026 Apnea