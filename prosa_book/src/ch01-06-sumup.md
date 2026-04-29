# Putting It All Together

Now that you're familiar with [Cargo-ProSA](ch01-01-cargo-prosa.md), [configuration](ch01-02-config.md), [observability](ch01-02-01-observability.md), and [running ProSA](ch01-02-04-run.md), let's walk through a complete example from scratch using the built-in processors.

## Prerequisites

You need [Rust and Cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html) installed, along with `cargo-prosa`:

```bash
cargo install cargo-prosa
```

## Create and scaffold a project

```bash
cargo prosa new my-first-prosa
cd my-first-prosa
```

This generates a Rust project with a `ProSA.toml` descriptor, a `build.rs`, and a `main.rs` that will be auto-generated from your ProSA configuration.

You can inspect the available components with:

```bash
cargo prosa list
```

This shows all discovered components: Main, TVF, Processors, and Adaptors.

## Add processors

Add a **stub** processor that will respond to requests, and an **injector** that will send requests:

```bash
cargo prosa add -n stub-1 -a StubParotAdaptor stub
cargo prosa add -n inj-1 -a InjDummyAdaptor inj
```

- `-n` sets the processor instance name (used in configuration)
- `-a` selects which adaptor to use

Your `ProSA.toml` file now contains the processor configuration. You can also edit this file manually.

## Generate and edit the configuration

Build the project and retrieve the generated configuration from _target/config.yml_ (or _target/config.toml_).

You can also generate a default configuration using `--dry_run` (see [Run ProSA](ch01-02-04-run.md) for details on this flag):

```bash
cargo run -- -c config.yaml --dry_run
```

Then edit `config.yaml` to wire the injector to the stub. The stub needs to declare a service name, and the injector needs to target that same service:

```yaml
name: "my-first-prosa"
observability:
  level: debug
  metrics:
    stdout:
      level: info
  traces:
    stdout:
      level: debug

stub_1:
  service_names:
    - "TEST_SERVICE"

inj_1:
  service_name: "TEST_SERVICE"
  max_speed: 1.0
```

This configures:
- The stub to respond to requests on `"TEST_SERVICE"`
- The injector to send 2 transactions per second to `"TEST_SERVICE"`
- [Observability](ch01-02-01-observability.md) output to stdout so you can see what's happening

## Run it

```bash
cargo run -- -n "MyFirstProSA" -c config.yaml
```

You should see log output showing:
1. ProSA starting with the configured name
2. The stub processor registering `TEST_SERVICE`
3. The injector discovering the service and starting to send transactions
4. Responses flowing back from the stub to the injector

Press `Ctrl+C` to stop (see [Graceful Shutdown](ch01-02-04-run.md#graceful-shutdown)).

## What's next?

You now know how to build and operate a ProSA instance. The next chapters cover how to develop your own components:
- **[Adaptor](ch02-00-adaptor.md)** — write custom protocol adaptors
- **[Processor](ch03-00-proc.md)** — write custom processors with their own business logic
