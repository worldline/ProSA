# Deployment Reference

## Container

Generate a container file for Docker or Podman:

```bash
cargo prosa container                          # Containerfile (Podman)
cargo prosa container --docker                 # Dockerfile
```

Options:
- `--image <IMAGE>` — base image (default: `debian:stable-slim`)
- `--builder <IMAGE>` — builder image for multi-stage build (e.g., `rust:latest`)
- `--package_manager <PM>` — package manager (default: `apt`)

The generated container file:
- Copies the binary, config, and service files
- Runs `--dry_run` validation before startup
- Includes labels: name, version, license, authors, description, documentation

### Multi-stage build

```bash
cargo prosa container --builder rust:latest --image debian:stable-slim
```

Compiles in the builder stage, copies only the binary to the final image.

## Local Installation

### Linux (systemd)

```bash
# User install
cargo prosa install --release --name my-prosa

# System install (requires sudo)
sudo -E $HOME/.cargo/bin/cargo prosa install --release --system --name my-prosa
```

User install paths:
| File | Path |
|------|------|
| Binary | `~/.local/bin/prosa-<name>` |
| Config | `~/.config/prosa/<name>/prosa.toml` |
| Service | `~/.config/systemd/user/<name>.service` |

System install paths:
| File | Path |
|------|------|
| Binary | `/usr/local/bin/prosa-<name>` |
| Config | `/etc/prosa/<name>/prosa.toml` |
| Service | `/etc/systemd/system/<name>.service` |

```bash
# Manage with systemctl
systemctl --user start <name>.service
systemctl --user status <name>.service
systemctl --user stop <name>.service
```

### macOS (launchd)

```bash
cargo prosa install --release --name my-prosa
```

Paths:
| File | Path |
|------|------|
| Binary | `~/.local/bin/prosa-<name>` |
| Config | `~/.config/prosa/<name>/prosa.toml` |
| Service | `~/Library/LaunchAgents/com.prosa.<name>.plist` |

```bash
launchctl load ~/Library/LaunchAgents/com.prosa.<name>.plist
launchctl unload ~/Library/LaunchAgents/com.prosa.<name>.plist
```

### Uninstall

```bash
cargo prosa uninstall --name my-prosa
cargo prosa uninstall --name my-prosa --purge   # also remove config
```

## Packages

### Debian (.deb)

Enable with `--deb` flag during project creation or update:

```bash
cargo prosa new my-prosa --deb
# or
cargo prosa update --deb
```

Build with [cargo-deb](https://crates.io/crates/cargo-deb). Package includes: release binary, default config, systemd service.

### RPM

Enable with `--rpm` flag:

```bash
cargo prosa new my-prosa --rpm
# or
cargo prosa update --rpm
```

Build with [cargo-generate-rpm](https://crates.io/crates/cargo-generate-rpm). Package includes: release binary, default config, systemd service.

## Running

```bash
# From source
cargo run -- -n "MyProSA" -c config.yaml

# From binary
target/release/my-prosa -n "MyProSA" -c config.yaml

# Validate before running
prosa_binary -c config.yaml --dry_run
```

## Feature Flags

Key features to consider for deployment:

| Feature | Purpose |
|---------|---------|
| `system-metrics` | RAM usage metrics (virtual/physical) |
| `prometheus` | Prometheus metrics endpoint |
| `http-proxy` | HTTP proxy support for streams |
| `openssl` | OpenSSL for TLS (default) |
| `openssl-vendored` | Statically linked OpenSSL |
| `queue` | Queue implementations |
