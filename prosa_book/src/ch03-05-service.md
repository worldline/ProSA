# Service

This part provides all the information you need to work with services within ProSA.

## Listening to a service

### Single

When your processor starts, you register it with the main task using [`add_proc()`](https://docs.rs/prosa/latest/prosa/core/proc/struct.ProcParam.html#method.add_proc).
After declaration, the main task gains access to a queue for sending service requests to your processor.
However, by default, your processor doesn't listen to any services.
To start listening to a specific service, call [`add_service_proc()`](https://docs.rs/prosa/latest/prosa/core/proc/struct.ProcParam.html#method.add_service_proc)

```rust,noplayground
# #[proc]
# struct MyProc {}
#
# #[proc]
# impl<A> Proc<A> for MyProc
# where
#     A: Adaptor + std::marker::Send + std::marker::Sync,
# {
    async fn internal_run(&mut self, name: String) -> Result<(), Box<dyn ProcError + Send + Sync>> {
        // Declare the processor
        self.proc.add_proc().await?;

        // Add all service to listen to
        self.proc
            .add_service_proc(vec![String::from("SERVICE_NAME")])
            .await?;

        loop {
            if let Some(msg) = self.internal_rx_queue.recv().await {
                match msg {
                    InternalMsg::Request(msg) => {
                        // Handle request from a declared service
                    }
                    InternalMsg::Response(msg) => {
                        // Handle response if the processor is registered
                    },
                    InternalMsg::Error(err) => {
                        // Handle errors as if they were responses
                    },
                    InternalMsg::Command(_) => todo!(),
                    InternalMsg::Config => todo!(),
                    InternalMsg::Service(table) => self.service = table,
                    InternalMsg::Shutdown => {
                        self.proc.remove_proc(None).await?;
                        return Ok(());
                    }
                }
            }
        }
    }
# }
```

### Multiple

When designing more complex processors, you may need to handle multiple subtasks, each requiring interactions with ProSA services.

In this case, you can declare multiple listener subtasks, each of which subscribes individually to its relevant service(s).

```rust,noplayground
# #[proc]
# struct MyProc {}
#
# #[proc]
# impl<A> Proc<A> for MyProc
# where
#     A: Adaptor + std::marker::Send + std::marker::Sync,
# {
    async fn internal_run(&mut self, name: String) -> Result<(), Box<dyn ProcError + Send + Sync>> {
        // Declare the processor
        self.proc.add_proc().await?;

        // Create a bus queue for subtask communication
        let (tx_queue, mut rx_queue) = tokio::sync::mpsc::channel(2048);
        let sub_proc = self.proc.clone();
        let subtask_id = 1;

        tokio::spawn(async move {
            // Register the processor with the main task
            sub_proc
                .add_proc_queue(tx_queue.clone(), subtask_id)
                .await?;

            // Register a service for this subtask only
            sub_proc.add_service(vec![String::from("SERVICE_NAME")], subtask_id).await?;

            // ...subtask logic...

            // Remove the service if it is no longer available
            sub_proc.remove_service(vec![String::from("SERVICE_NAME")], subtask_id).await?;

            loop {
                // Local service table for the task
                let service = ServiceTable::default();
                if let Some(msg) = rx_queue.recv().await {
                    match msg {
                        InternalMsg::Request(msg) => {
                            // Handle request for this subtask
                        }
                        InternalMsg::Response(msg) => {
                            // Handle response (must have registered the processor)
                        },
                        InternalMsg::Error(err) => {
                            // Handle errors as if they were responses
                        },
                        InternalMsg::Command(_) => todo!(),
                        InternalMsg::Config => todo!(),
                        InternalMsg::Service(table) => service = table,
                        InternalMsg::Shutdown => {
                            self.proc.remove_proc(None).await?;
                            return Ok(());
                        }
                    }
                }
            }
        })

        loop {
            if let Some(msg) = self.internal_rx_queue.recv().await {
                match msg {
                    InternalMsg::Request(msg) => {
                        // Handle request from a declared service
                    }
                    InternalMsg::Response(msg) => {
                        // Handle response if the processor is registered
                    },
                    InternalMsg::Error(err) => {
                        // Handle errors as if they were responses
                    },
                    InternalMsg::Command(_) => todo!(),
                    InternalMsg::Config => todo!(),
                    InternalMsg::Service(table) => self.service = table,
                    InternalMsg::Shutdown => {
                        self.proc.remove_proc(None).await?;
                        return Ok(());
                    }
                }
            }
        }
    }
# }
```

## Sending messages

### Single

Even if your processor only sends messages, it must be registered to receive responses and errors for your requests using [`add_proc()`](https://docs.rs/prosa/latest/prosa/core/proc/struct.ProcParam.html#method.add_proc).
After that, you are free to call any services.

```rust,noplayground
# #[proc]
# struct MyProc {}
#
# #[proc]
# impl<A> Proc<A> for MyProc
# where
#     A: Adaptor + std::marker::Send + std::marker::Sync,
# {
    async fn internal_run(&mut self, name: String) -> Result<(), Box<dyn ProcError + Send + Sync>> {
        // Register the processor
        self.proc.add_proc().await?;

        // Wait for the service table before sending messages to a service
        loop {
            if let Some(msg) = self.internal_rx_queue.recv().await {
                match msg {
                    InternalMsg::Request(msg) => {
                        // Handle incoming requests if needed
                    }
                    InternalMsg::Response(msg) => {
                        // Handle response
                    },
                    InternalMsg::Error(err) => {
                        // Handle errors
                    },
                    InternalMsg::Command(_) => todo!(),
                    InternalMsg::Config => todo!(),
                    InternalMsg::Service(table) => self.service = table,
                    InternalMsg::Shutdown => {
                        self.proc.remove_proc(None).await?;
                        return Ok(());
                    }
                }
            }

            // Attempt to send a message if the service is available
            if let Some(service) = self.service.get_proc_service("SERVICE_NAME") {
                let trans = RequestMsg::new(
                    String::from("SERVICE_NAME"),
                    M::default(),
                    self.proc.get_service_queue()
                );
                service.proc_queue.send(InternalMsg::Request(trans)).await?;
            }
        }

        Ok(())
    }
# }
```

### Multiple

If you have multiple subtasks, each must use its own queue to ensure responses are routed to the correct subtask.
The logic is similar to single senders, but you specify the queue when sending messages.

```rust,noplayground
# #[proc]
# struct MyProc {}
#
# #[proc]
# impl<A> Proc<A> for MyProc
# where
#     A: Adaptor + std::marker::Send + std::marker::Sync,
# {
    async fn internal_run(&mut self, name: String) -> Result<(), Box<dyn ProcError + Send + Sync>> {
        // Register the processor
        self.proc.add_proc().await?;

        // Create a queue for subtask communication
        let (tx_queue, mut rx_queue) = tokio::sync::mpsc::channel(2048);
        let tx_msg_queue = tx_queue.clone();
        let sub_proc = self.proc.clone();
        let subtask_id = 1;

        tokio::spawn(async move {
            // Register the processor to the main task
            sub_proc
                .add_proc_queue(tx_queue.clone(), subtask_id)
                .await?;

            loop {
                if let Some(msg) = rx_queue.recv().await {
                    match msg {
                        InternalMsg::Request(msg) => todo!()
                        InternalMsg::Response(msg) => {
                            // Handle response for this subtask
                        },
                        InternalMsg::Error(err) => {
                            // Handle errors for this subtask
                        },
                        InternalMsg::Command(_) => todo!(),
                        InternalMsg::Config => todo!(),
                        InternalMsg::Service(table) => self.service = table,
                        InternalMsg::Shutdown => {
                            self.proc.remove_proc(None).await?;
                            return Ok(());
                        }
                    }
                }
            }

            // Attempt to send a message if the service is available
            if let Some(service) = self.service.get_proc_service("SERVICE_NAME") {
                let trans = RequestMsg::new(
                    String::from("SERVICE_NAME"),
                    M::default(),
                    tx_msg_queue.clone()
                );
                service.proc_queue.send(InternalMsg::Request(trans)).await?;
            }
        })

        Ok(())
    }
# }
```
