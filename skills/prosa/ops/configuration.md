# Configuration Reference

ProSA uses YAML configuration files, with environment variable overrides.

## Single Configuration File

Default path: `/etc/prosa.yml` (or specified via `-c` flag).

```yaml
name: "prosa-name"
observability:
  level: debug
  metrics:
    stdout:
      level: info
  traces:
    stdout:
      level: debug
  logs:
    stdout:
      level: debug

proc-1:
  # Processor 1 settings (matches #[proc_settings] struct fields)
  service_names:
    - "SERVICE_A"
  timeout: "5s"

proc-2:
  # Processor 2 settings
```

## Multiple Configuration Files

Point `-c` to a directory. All YAML/TOML files in the directory are merged:

```yaml
# /etc/myprosa/main.yml
name: "prosa-name"
observability:
  level: debug
```

```yaml
# /etc/myprosa/proc_1.yml
proc-1:
  service_names:
    - "SERVICE_A"
```

## Environment Variables

Override configuration values via environment variables:

```bash
PROSA_NAME="my-prosa" prosa_binary -c config.yaml
```

## Generating Default Configuration

Use `--dry_run` to generate a starter configuration file with all processor defaults:

```bash
prosa_binary -c default_config.yaml --dry_run
```

If the file does not exist, it writes one with default values from all registered processors.

## Runtime CLI Options

```
--dry_run                   Validate config without starting
-c, --config <PATH>         Config file or directory [default: prosa.yml]
-n, --name <NAME>           Override ProSA name
-t, --worker_threads <N>    Tokio runtime threads for Main task [default: 1]
-V                          Short version
--version                   Detailed version with all components
-h, --help                  Print help
```

## Version Output

```bash
$ prosa_binary --version
prosa 0.1.0 - core::main::MainProc = { crate = prosa, version = 0.2.0 }
  inj
    Processor: inj::proc::InjProc = { crate = prosa, version = 0.2.0 }
    Adaptor  : inj::adaptor::InjDummyAdaptor = { crate = prosa, version = 0.2.0 }
  stub
    Processor: stub::proc::StubProc = { crate = prosa, version = 0.2.0 }
    Adaptor  : stub::adaptor::StubParotAdaptor = { crate = prosa, version = 0.2.0 }
```

Shows the Main processor type, every processor instance, and their adaptors with crate versions.

## Processor Settings (auto-added by `#[proc_settings]`)

Every processor settings struct gets these fields automatically:

- `adaptor_config_path` — path to adaptor-specific config file
- `proc_restart_duration_period` — seconds between restart attempts
- `proc_max_restart_period` — max seconds to wait between restarts

```yaml
proc-1:
  adaptor_config_path: "/etc/prosa/adaptor1.yml"
  proc_restart_duration_period: 50
  proc_max_restart_period: 300
  # ... your custom fields
```

## Graceful Shutdown

1. SIGINT (Ctrl+C) caught by Main processor
2. `Shutdown` message sent to all registered processors
3. Each processor calls `adaptor.terminate()` to release resources
4. Each processor deregisters with `remove_proc()`
5. Main exits once all processors have stopped
