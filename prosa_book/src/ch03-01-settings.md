# Processor settings

As you saw in the [cargo-prosa](ch01-01-cargo-prosa.md) chapter, every processor has a configuration object attached to it.
You'll specify your processor settings object when you create your processor in the next chapter.

> `Settings` is the top-level configuration object, while `ProcSettings` is specific to processors.

## Loading

Use [`ProsaConfig`](https://docs.rs/prosa/latest/prosa/core/settings/struct.ProsaConfig.html) to load a ProSA configuration from a file or a directory:

```rust,ignore
use prosa::core::settings::ProsaConfig;

let config = ProsaConfig::from_path("prosa.yml")?;
let settings = config.try_deserialize::<MyRunSettings>()?;
```

`ProsaConfig::from_path()` uses the same configuration loading rules as ProSA itself: the path can be a single configuration file or a directory recursively containing `yml`, `yaml`, or `toml` files, and `PROSA_*` environment variables are applied on top of the file sources.

When the main task notifies a processor about a configuration change, the message contains the same `ProsaConfig` wrapper. Processors should reload their own section with `config.reload_proc::<MyProcSettings>(self.proc.as_ref(), &adaptor)`, which deserializes the processor settings and reloads the adaptor configuration in one step. If it returns a `ConfigError`, the processor should log the failure with its name and keep using its current settings.
If the processor section has `adaptor_config_path`, `ProsaConfig` also loads that adaptor configuration and watches it as part of the global configuration reload flow.

## Creation

To create a processor settings, declare a `struct` and use the [`proc_settings`](https://docs.rs/prosa/latest/prosa/core/proc/attr.proc_settings.html) macro.
This macro adds necessary members to your struct and implements the [`ProcSettings`](https://docs.rs/prosa/latest/prosa/core/proc/trait.ProcSettings.html) trait for you.

> From these additional members, you will be able to obtain your adapter configuration and processor restart policy.

You can specify them as configurations for your processor like this:
```yaml
proc:
    adaptor_config_path: /etc/adaptor_path.yaml
    proc_restart_duration_period: 50
    proc_max_restart_period: 300
    my_param: "test"
```

And declare your settings like this in Rust:
```rust,ignore
use serde::{Deserialize, Serialize};

#[proc_settings]
#[derive(Debug, Deserialize, Serialize)]
pub struct MySettings {
    my_param: String,
}
```

## Implementing Default

Since the `proc_settings` macro adds fields to your struct, it can be tricky to manually implement a default value.
Fortunately, the macro also supports a custom `Default` implementation that incorporates all required fields:
```rust,ignore
#[proc_settings]
impl Default for MySettings {
    fn default() -> Self {
        MySettings {
            my_param: "default param".into(),
        }
    }
}
```

By implementing `Default` for your settings, you can then create a `new` function that uses default parameters, for example:
```rust,ignore
impl MySettings {
    pub fn new(my_param: String) -> MySettings {
        MySettings {
            my_param,
            ..Default::default()
        }
    }
}
```
