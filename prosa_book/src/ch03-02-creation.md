# Processor creation

A processor in ProSA is an autonomous routine executed within its own [thread(s)](ch03-08-threads.md).
Processor interact with each other through internal TVF messages.

## Creation

The [Proc module](https://docs.rs/prosa/latest/prosa/core/proc/index.html) contains everything you need to create a processor, along with an example processor and configuration.

To create a processor, use the [proc macro](https://docs.rs/prosa/latest/prosa/core/proc/attr.proc.html), and implement the [`Proc`](https://docs.rs/prosa/latest/prosa/core/proc/trait.Proc.html) trait.

Given a settings struct named `MyProcSettings` for your processor, your processor struct declaration would look like this:
```rust,noplayground
#[proc(settings = MyProcSettings)]
pub struct MyProc { /* No members here */ }
```

> The macro currently does not allow you to add members directly to your struct.

This is usually not an issue, as you can instantiate and use variables within `internal_run()` (the main loop of the processor).

You can still declare methods on your struct as needed:
```rust,noplayground
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
```rust,noplayground
#[proc]
impl<A> Proc<A> for MyProc
where
    A: Adaptor + std::marker::Send + std::marker::Sync,
{
    async fn internal_run(&mut self, name: String) -> Result<(), Box<dyn ProcError + Send + Sync>> {
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
                    InternalMsg::Command(_) => todo!(),
                    InternalMsg::Config => todo!(),
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

The generic parameter `A` represents the adaptor type your processor uses.
Specify in the _where_ clause which traits your adaptor must implement (commonly, [`Adaptor`](https://docs.rs/prosa/latest/prosa/core/adaptor/trait.Adaptor.html) plus `Send` and `Sync`)

### Specific TVF

Sometimes, you may want your processor to handle only specific TVF objects, possibly to optimize data handling performance or to provide dedicated logic.
In these cases, explicitly implement the `Proc` trait for your processor, parameterized by the specific TVF type:

```rust,noplayground
#[proc]
impl<A> Proc<A> for MyProc<SimpleStringTvf>
where
    A: Adaptor + std::marker::Send + std::marker::Sync,
{
    async fn internal_run(&mut self, name: String) -> Result<(), Box<dyn ProcError + Send + Sync>> {
        // Custom handling for SimpleStringTvf
    }
}
```
