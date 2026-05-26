# cargo-prosa CLI Reference

`cargo-prosa` is a Cargo subcommand for scaffolding, building, and packaging ProSA applications.

## Install

```bash
cargo install cargo-prosa
cargo prosa --help
```

## Commands

### `cargo prosa new <PATH>` — Create a new ProSA package

```bash
cargo prosa new my-prosa
cargo prosa new my-prosa --deb    # with Debian package support
cargo prosa new my-prosa --rpm    # with RPM package support
```

Creates a Rust project with `ProSA.toml`, `build.rs`, `main.rs`, and `Cargo.toml` pre-configured.

### `cargo prosa init` — Initialize ProSA in existing directory

```bash
cargo prosa init
cargo prosa init --deb --rpm
```

### `cargo prosa update` — Update skeleton files

```bash
cargo prosa update
```

Updates generated files (`build.rs`, `main.rs`) to the latest cargo-prosa version.

### `cargo prosa add <PROCESSOR>` — Add a processor

```bash
cargo prosa add stub
cargo prosa add -n stub-1 -a StubParotAdaptor stub
```

Flags:
- `-n <NAME>` — instance name for the processor
- `-a <ADAPTOR>` — adaptor to use with the processor

Updates `ProSA.toml` with the new processor entry.

### `cargo prosa remove <PROCESSORS...>` — Remove processors

```bash
cargo prosa remove stub-1
cargo prosa remove proc1 proc2
```

### `cargo prosa main <MAIN>` — Change main processor

```bash
cargo prosa main MainProc
```

Default: `prosa::core::main::MainProc`.

### `cargo prosa tvf <TVF>` — Change TVF type

```bash
cargo prosa tvf SimpleStringTvf
```

Default: `prosa_utils::msg::simple_string_tvf::SimpleStringTvf`.

### `cargo prosa list` — List available components

```bash
cargo prosa list
```

Lists all discoverable components from dependencies:
- **Main** — main task implementations
- **TVF** — internal message format types
- **Processor/Settings** — processors and their settings
- **Adaptor** — adaptors available for each processor

### `cargo prosa container` — Generate container file

```bash
cargo prosa container                          # Containerfile (Podman)
cargo prosa container --docker                 # Dockerfile
cargo prosa container --image debian:stable-slim
cargo prosa container --builder rust:latest    # multi-stage build
cargo prosa container --package_manager apt
```

Generates a container file with optional multi-stage build support. Validates with `--dry_run` before container startup.

### `cargo prosa install` — Install on host

```bash
cargo prosa install                    # user install
cargo prosa install --release          # release build
cargo prosa install --name dummy       # named instance
cargo prosa install --system           # system-wide (requires sudo)
cargo prosa install --dry_run          # simulate
```

#### Linux (systemd)

User install creates:
- Binary: `~/.local/bin/prosa-<name>`
- Config: `~/.config/prosa/<name>/prosa.toml`
- Service: `~/.config/systemd/user/<name>.service`

System install creates:
- Binary: `/usr/local/bin/prosa-<name>`
- Config: `/etc/prosa/<name>/prosa.toml`
- Service: `/etc/systemd/system/<name>.service`

```bash
# Manage with systemctl
systemctl --user status <name>.service
systemctl --user start <name>.service
```

#### macOS (launchd)

Creates:
- Binary: `~/.local/bin/prosa-<name>`
- Config: `~/.config/prosa/<name>/prosa.toml`
- Service: `~/Library/LaunchAgents/com.prosa.<name>.plist`

```bash
launchctl load ~/Library/LaunchAgents/com.prosa.<name>.plist
launchctl unload ~/Library/LaunchAgents/com.prosa.<name>.plist
```

### `cargo prosa uninstall` — Remove installation

```bash
cargo prosa uninstall --name dummy
cargo prosa uninstall --purge          # also remove config
```

### `cargo prosa completion <SHELL>` — Shell completions

```bash
cargo prosa completion bash
cargo prosa completion zsh
cargo prosa completion fish
```

## Global Flag

All commands support `--dry_run` to preview changes without applying them.

## ProSA.toml

The descriptor file managed by cargo-prosa:

```toml
[prosa]
main = "prosa::core::main::MainProc"
tvf = "prosa_utils::msg::simple_string_tvf::SimpleStringTvf"

[[proc]]
name = "stub-1"
proc_name = "stub"
proc = "prosa::stub::proc::StubProc"
adaptor = "prosa::stub::adaptor::StubParotAdaptor"
```
