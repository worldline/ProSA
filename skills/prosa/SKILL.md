---
name: prosa
description: >
  ProSA (Protocol Service Adaptor) Rust framework assistant covering development,
  operations, and diagnostics. Use when working with ProSA processors (#[proc] macro),
  adaptors (#[derive(Adaptor)]), settings (#[proc_settings], #[settings]), error types
  (ProcError trait), services, events, I/O handlers (#[io] macro), TVF messages (tvf! macro),
  cargo-prosa CLI, deployment, observability, configuration, or troubleshooting.
  Also use when the user mentions ProSA-related concepts like service bus, internal_run,
  InternalMsg, ServiceTable, MainProc, StubProc, InjProc, MaybeAsync, or asks about
  ProSA architecture and patterns.
license: LGPL-3.0-or-later
compatibility: Requires Rust 2024 edition, ProSA framework
metadata:
  author: ProSA Contributors
  tags: prosa, rust, soa, framework, processor, adaptor
---

# ProSA Assistant

Generate correct, production-ready code and guidance for the ProSA framework.
This skill covers three areas: **development**, **operations**, and **diagnostics**.

## Development

Create processors, adaptors, settings, error types, services, I/O handlers, and TVF messages.

### Guided Workflow: Creating a Processor

When a user asks to create a processor, follow these steps in order:

1. **Gather Requirements** — What does it do? What services does it listen to / call? Network I/O? Configuration? Timeout tracking? Flow control?
2. **Create Error Type** — Read `dev/processor-pattern.md` section 1
3. **Create Settings** — Read `dev/processor-pattern.md` section 2
4. **Create Adaptor** — Read `dev/adaptor-patterns.md` for the appropriate pattern
5. **Create Processor** — Read `dev/processor-pattern.md` sections 3-4
6. **Declare in Cargo.toml** — `[package.metadata.prosa.my_proc]` block
7. **Configuration YAML** — Processor settings under `procs:`

### Dev Reference Files

| File | When to read |
|------|-------------|
| `dev/processor-pattern.md` | Creating a processor, settings, error type, or main binary |
| `dev/adaptor-patterns.md` | Creating any adaptor (simple, custom trait, Stub, Inj) |
| `dev/events-services.md` | Using PendingMsgs, Regulator, multi-subtask, sender-only pattern |
| `dev/io-tvf.md` | Network I/O (listener/stream), TVF message creation, `tvf!` macro |

What `cargo-prosa` already handles (don't duplicate): project scaffolding (`cargo prosa new`), ProSA.toml management, build.rs/main.rs templates, dependency wiring.

## Operations

Deploy, configure, monitor, and manage ProSA applications.

### Ops Reference Files

| File | When to read |
|------|-------------|
| `ops/cargo-prosa-cli.md` | Using `cargo prosa` commands (new, add, install, container, etc.) |
| `ops/configuration.md` | ProSA.toml, YAML config, env vars, hot-reload |
| `ops/observability.md` | Metrics (Prometheus, OTLP), traces, logs, Grafana |
| `ops/deployment.md` | Container generation, systemd/launchd, install/uninstall, cloud |
| `ops/ssl-network.md` | SSL/TLS, listener, stream, proxy configuration |

## Diagnostics

Troubleshoot, debug, and tune ProSA applications.

### Diag Reference Files

| File | When to read |
|------|-------------|
| `diag/health-status.md` | Processor states (green/orange/grey/red), service availability |
| `diag/error-recovery.md` | ProcError, restart logic, backoff, common error patterns |
| `diag/troubleshooting.md` | Config validation (--dry_run), common pitfalls, log analysis |
| `diag/performance.md` | PendingMsgs tuning, Regulator flow control, queue diagnostics |

## Critical Rules

These rules are non-negotiable for all generated ProSA code:

1. **Never use `.unwrap()`** — `clippy::unwrap_used = "deny"` across all ProSA crates. Use `?` operator, `.expect("reason")` only when truly infallible, or proper error handling.

2. **Handle ALL `InternalMsg` variants** — Every `match msg` must handle: `Request`, `Response`, `Error`, `Command`, `Config`, `Service`, `Shutdown`. Use empty bodies `{}` for unused variants, never omit them.

3. **Shutdown sequence** — Always call `adaptor.terminate()` **before** `self.proc.remove_proc(None).await?`, then `return Ok(())`.

4. **Register before loop** — `self.proc.add_proc().await?` must be called before the main event loop.

5. **`extern crate self as prosa;`** — Required only when writing processors inside the `prosa` crate itself. NOT needed in external crates.

6. **Rust 2024 edition** — Use current Rust idioms.

7. **Error handling** — Always implement `ProcError` for custom error types. Mark IO/network errors as `recoverable = true`, configuration errors as `recoverable = false`.

8. **Warnings are errors** — CI uses `RUSTFLAGS=-Dwarnings`. No unused imports, dead code, or clippy warnings.
