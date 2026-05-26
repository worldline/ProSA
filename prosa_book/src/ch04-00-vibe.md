# Vibe ProSA

ProSA provides an AI coding skill that helps you develop, deploy, and troubleshoot ProSA applications through natural language. The skill is packaged as an [Agent Skills](https://agentskills.io/) plugin, compatible with Claude Code, GitHub Copilot, OpenAI Codex, Cursor, Windsurf, Gemini CLI, and other AI coding tools.

While [Cargo-ProSA](ch01-01-cargo-prosa.md) handles project scaffolding and deployment, the skill covers three areas: **development** (generating Rust implementation code), **operations** (configuration, deployment, monitoring), and **diagnostics** (troubleshooting, performance tuning).

## Install

The skill is distributed as an npm package. Install it in your project:

```bash
npm install @anthropic-ai/prosa-skill
```

The package follows the [Agent Skills](https://agentskills.io/) open standard. AI coding tools that support this standard will discover the skill automatically from `node_modules/`.

For Claude Code specifically, the package also includes a `.claude-plugin/plugin.json` manifest for plugin discovery.

## Skill Organization

The skill is a single index (`skills/prosa/SKILL.md`) with three sections, each backed by detailed reference files:

### Development

Create processors, adaptors, settings, error types, and other ProSA components:

- **Processor creation** — `#[proc]` struct, `Proc` trait implementation with the full `internal_run` event loop
- **Adaptor creation** — simple adaptors, custom adaptor traits, `StubAdaptor`, `InjAdaptor`
- **Settings** — `#[proc_settings]` / `#[settings]` with serde and default values
- **Error types** — custom error enums with `thiserror`, `ProcError` trait, `ServiceError` conversion
- **Service patterns** — single service, multi-subtask, sender-only
- **Event handling** — `PendingMsgs` timeout tracking, `Regulator` flow control
- **I/O handlers** — `#[io]` macro, `StreamListener`, `Stream`
- **TVF messages** — `tvf!` macro, manual `put_*/get_*`

### Operations

Deploy, configure, and monitor ProSA applications:

- **Cargo-ProSA CLI** — all commands (`new`, `add`, `install`, `container`, etc.) with flags and examples
- **Configuration** — `ProSA.toml`, YAML settings, environment variables, hot-reload
- **Observability** — Prometheus, OTLP (gRPC/HTTP), Grafana Cloud, stdout; metrics, traces, logs
- **Deployment** — container generation (Docker/Podman), systemd/launchd services, DEB/RPM packages
- **SSL & Network** — TLS configuration, listener/stream settings, proxy, mTLS

### Diagnostics

Troubleshoot issues and tune performance:

- **Health & Status** — processor states (green/orange/grey/red), service availability, node graph
- **Error Recovery** — `ProcError` trait, automatic restart, backoff, `ServiceError` handling
- **Troubleshooting** — `--dry_run` validation, common pitfalls, log analysis, trace correlation
- **Performance** — `PendingMsgs` tuning, `Regulator` flow control, queue diagnostics, scaling patterns

## Usage

Simply describe what you want in natural language. The skill triggers automatically when your prompt involves ProSA components.

### Development examples

> Create a processor called PaymentRouter that receives requests on service PAYMENT_ROUTE, looks up the destination from TVF field 1, and forwards the request to the appropriate service. It should track pending messages with a 5-second timeout.

The AI assistant will generate:
1. An error type with `ProcError` implementation
2. A settings struct with `#[proc_settings]`
3. An adaptor trait and default implementation
4. The processor with `#[proc]` and a complete `internal_run` loop
5. The `Cargo.toml` metadata declaration

> Create a StubAdaptor that simulates a card authorization service. It should approve transactions under 500 and decline anything above.

### Operations examples

> I want to create a new ProSA project with a stub processor, generate a Dockerfile for it, and configure Prometheus metrics.

> How do I install my ProSA as a system service on Linux with release optimization?

> How do I configure ProSA to send metrics, traces, and logs to Grafana Cloud via OTLP?

### Diagnostics examples

> My processor keeps restarting every few seconds and shows orange in the Grafana dashboard. How do I diagnose this?

> I'm getting ServiceError::Timeout errors. How do I investigate and tune the performance?

## What the skill enforces

The generated code follows ProSA conventions:

- **No `.unwrap()`** — the project bans it via `clippy::unwrap_used = "deny"`
- **All `InternalMsg` variants handled** — `Request`, `Response`, `Error`, `Command`, `Config`, `Service`, `Shutdown`
- **Proper shutdown sequence** — `adaptor.terminate()` before `proc.remove_proc(None).await?`
- **Registration before loop** — `proc.add_proc().await?` must precede the main event loop
- **Proper error handling** — `ProcError` trait with recoverable/fatal distinction
