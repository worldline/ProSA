# Processor settings

As you saw in the [cargo-prosa](ch01-01-cargo-prosa.md) chapter, every processor has a configuration object attached to it.
You'll specify your processor settings object when you create your processor in the next chapter.

> `Settings` is the top-level configuration object, while `ProcSettings` is specific to processors.

## Creation

To create a processor settings, declare a `struct` and use the [`proc_settings`](https://docs.rs/prosa/latest/prosa/core/proc/attr.proc_settings.html) macro.
This macro adds necessary members to your struct and implements the [`ProcSettings`](https://docs.rs/prosa/latest/prosa/core/proc/trait.ProcSettings.html) trait for you.

> From these additional members, you will be able to obtain your adapter configuration and processor restart policy.

```rust,noplayground
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
```rust,noplayground
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
```rust,noplayground
impl MySettings {
    pub fn new(my_param: String) -> MySettings {
        MySettings {
            my_param,
            ..Default::default()
        }
    }
}
```
