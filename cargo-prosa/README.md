# cargo-prosa

ProSA is a framework that handles processors organized around a service bus.
As such, ProSA needs to be built from internal or external Processors/Adaptor/Main.

_cargo-prosa_ is a utility to package and deliver a builded ProSA.
This builder is packaged within cargo as a custom command to be well integrated with the Rust ecosystem.

## Install

To use it, you need to install it within Cargo.
```bash
cargo install cargo-prosa
```

## Use

Create your own ProSA (work like cargo):
```bash
cargo prosa new my-prosa
# or from an existing folder, init it
cargo prosa init
```

_cargo-prosa_ is meant to evolve in the future.
So maybe new things will be introduced.
To update your model, you can update the generated file with `cargo prosa update`.

If you have different main/tvf, select them:
```bash
cargo prosa main MainProc
cargo prosa tvf SimpleStringTvf
```

Add your dependencies and your processor with its adaptor name
```bash
cargo add prosa
cargo prosa add -n stub-1 -a StubParotAdaptor stub
```

Once your ProSA is specified, the file _ProSA.toml_ will contain the configuration.
This file can be edited manually if you want.

Your project uses a _build.rs_/_main.rs_ to create a binary that you can use.


## Configuration

Keep in mind that you also need to have a settings file.
A `target/config.yml` and `target/config.toml` will be generated when building.

But you can initiate a default one with:
```bash
cargo run -- -c default_config.yaml --dry_run
```

A configuration file contains:
 - name: Name of your ProSA
 - observability: Configuration of log/trace/metrics
 - a map of processor name -> their settings

## Run

When your ProSA is built, you can deploy like any Rust binary.
So you'll find it in the target folder.

And you can run it:
```bash
cargo run -- -n "MyBuiltProSA" -c default_config.yaml
# or with binary
target/debug/my-prosa -n "MyBuiltProSA" -c default_config.yaml
```

## Deploy

This builder offer you several possibilities to deploy your ProSA.
The goal is to use the easiest method of a plateform to run your application.

### Container

Containerization will allow you to build and load ProSA in an image:
```bash
# Generate a Containerfile
cargo prosa container
# Generate a Dockerfile
cargo prosa container --docker
```

For your own needs, you can:
 - Select from which image the container need to be build `--image debian:stable-slim`
 - Along that you may have to specify the package manager use to install mandatory packages `--package_manager apt`
 - If you want to compile ProSA through a builder, you can specify it with `--builder rust:latest`. A multi stage container file will be created.

### Deb package

Deb package can be created with the [cargo-deb](https://crates.io/crates/cargo-deb) crate.

To enable this feature, _create_, _init_ or _update_ your ProSA with the option `--deb`.
It'll add every needed properties to generate a deb package.

The deb package will include the released binary, a default configuration file, and a systemd service file.

### RPM package

RPM (Red Hat Package Manager) package can be created with the [cargo-generate-rpm](https://crates.io/crates/cargo-generate-rpm) crate.

To enable this feature, _create_, _init_ or _update_ your ProSA with the option `--rpm`.
It'll add every needed properties to generate an rpm package.

The rpm package will include the released binary, a default configuration file, and a systemd service file.
