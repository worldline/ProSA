# Processor creation

A processor in ProSA is an autonomous routine executed within its own [thread(s)](ch03-08-threads.md).
Processor interact with each other through internal TVF messages.

## Creation

The [Proc module](https://docs.rs/prosa/latest/prosa/core/proc/index.html) contains everything you need to create a processor, along with an example processor and configuration.

To create a processor, use the [proc macro](https://docs.rs/prosa/latest/prosa/core/proc/attr.proc.html), and implement the [`Proc`](https://docs.rs/prosa/latest/prosa/core/proc/trait.Proc.html) trait.

Given a settings struct named `MyProcSettings` for your processor, your processor struct declaration would look like this:
```rust,ignore
#[proc(settings = MyProcSettings)]
pub struct MyProc { /* No members here */ }
```

> The macro currently does not allow you to add members directly to your struct.

This is usually not an issue, as you can instantiate and use variables within `internal_run()` (the main loop of the processor).

You can still declare methods on your struct as needed:
```rust,ignore
#[proc]
impl MyProc
{
    fn internal_func() {
        // You can declare additional helper functions here
    }
}
```

Finally, implement the [`Proc`](https://docs.rs/prosa/latest/prosa/core/proc/trait.Proc.html) trait.

Here's an example skeleton:
```rust,ignore
#[proc]
impl<A> Proc<A> for MyProc
where
    A: Adaptor + std::marker::Send + std::marker::Sync,
{
    async fn internal_run(&mut self) -> Result<(), Box<dyn ProcError + Send + Sync>> {
        // TODO: Initialize your adaptor here

        // Register the processor if ready to run
        self.proc.add_proc().await?;

        loop {
            if let Some(msg) = self.internal_rx_queue.recv().await {
                match msg {
                    InternalMsg::Request(msg) => {
                        // TODO: process the request
                    }
                    InternalMsg::Response(msg) => {
                        // TODO: process the response
                    }
                    InternalMsg::Error(err) => {
                        // TODO: process the error
                    }
                    InternalMsg::Config(config) => {
                        if let Some(settings) =
                            config.reload_proc::<MyProcSettings>(self.proc.as_ref(), &adaptor)
                        {
                            // TODO: apply the difference between `settings` and `self.settings`
                            self.settings = settings;
                        }
                    },
                    InternalMsg::Service(table) => self.service = table,
                    InternalMsg::Shutdown => {
                        adaptor.terminate();
                        self.proc.remove_proc(None).await?;
                        return Ok(());
                    }
                }
            }
        }
    }
}
```

When receiving `InternalMsg::Config(config)`, call `config.reload_proc::<MyProcSettings>(self.proc.as_ref(), &adaptor)` to deserialize the section matching the processor configuration key (the processor name with `-` replaced by `_`) into the processor settings type, and reload the adaptor configuration in the same step. It returns `None` if either fails, so the processor keeps running on its current configuration. A processor that has no configuration section keeps the settings it was created with, which is reported at debug level; anything else is logged as a warning.

The main task sends this message once when the processor registers, then on every reload that changes its own configuration section. So compare the new settings with the ones the processor already holds, and apply only the difference.

The generic parameter `A` represents the adaptor type your processor uses.
Specify in the _where_ clause which traits your adaptor must implement (commonly, [`Adaptor`](https://docs.rs/prosa/latest/prosa/core/adaptor/trait.Adaptor.html) plus `Send` and `Sync`)

### Specific TVF

Sometimes, you may want your processor to handle only specific TVF objects, possibly to optimize data handling performance or to provide dedicated logic.
In these cases, explicitly implement the `Proc` trait for your processor, parameterized by the specific TVF type:

```rust,ignore
#[proc]
impl<A> Proc<A> for MyProc<SimpleStringTvf>
where
    A: Adaptor + std::marker::Send + std::marker::Sync,
{
    async fn internal_run(&mut self) -> Result<(), Box<dyn ProcError + Send + Sync>> {
        // Custom handling for SimpleStringTvf
    }
}
```
