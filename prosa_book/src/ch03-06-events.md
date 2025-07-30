# Events

ProSA, being a transactional framework, makes events extremely useful when developing a processor.

In the next sections, we'll go over all event-based objects provided by ProSA.

## Messages with timeout - `PendingMsgs`

Your processor should handle timeouts for transactions in order to drop them if they cannot be processed in time.
That's the purpose of the [`PendingMsgs`](https://docs.rs/prosa/latest/prosa/event/pending/struct.PendingMsgs.html) object.

There are three important methods you need to use for this object:
- [`push()`](https://docs.rs/prosa/latest/prosa/event/pending/struct.PendingMsgs.html#method.push) Add your message to be monitored for timeouts.
- [`pull_msg()`](https://docs.rs/prosa/latest/prosa/event/pending/struct.PendingMsgs.html#method.pull) Remove your message when you have received its response and no longer need to check its timeout.
- [`pull()`](https://docs.rs/prosa/latest/prosa/event/pending/struct.PendingMsgs.html#method.pull) Async method to retrieve all messages that have expired (timed out).

```rust,noplayground
# #[proc]
# struct MyProc {}
#
# #[proc]
# impl<A> Proc<A> for MyProc
# where
#     A: Default + Adaptor + std::marker::Send + std::marker::Sync,
# {
    async fn internal_run(
        &mut self,
        _name: String,
    ) -> Result<(), Box<dyn ProcError + Send + Sync>> {
        let mut adaptor = A::default();
        self.proc.add_proc().await?;
        self.proc
            .add_service_proc(vec![String::from("PROC_TEST")])
            .await?;
        let mut interval = time::interval(time::Duration::from_secs(4));
        let mut pending_msgs: PendingMsgs<RequestMsg<M>, M> = Default::default();
        loop {
            tokio::select! {
                Some(msg) = self.internal_rx_queue.recv() => {
                    match msg {
                        InternalMsg::Request(msg) => {
                            info!("Proc {} receive a request: {:?}", self.get_proc_id(), msg);

                            // Add to pending messages to track timeout
                            pending_msgs.push(msg, Duration::from_millis(200));
                        },
                        InternalMsg::Response(msg) => {
                            let _enter = msg.enter_span();
                            // Try to retrieve original request; if it already timed out, this returns None
                            let original_request: Option<RequestMsg<SimpleStringTvf>> = pending_msgs.pull_msg(msg.get_id());
                            info!("Proc {} receive a response: {:?}, from original request {:?}", self.get_proc_id(), msg, original_request);
                        },
                        InternalMsg::Error(err) => {
                            let _enter = err.enter_span();
                            info!("Proc {} receive an error: {:?}", self.get_proc_id(), err);
                        },
                        InternalMsg::Command(_) => todo!(),
                        InternalMsg::Config => todo!(),
                        InternalMsg::Service(table) => {
                            debug!("New service table received:\n{}\n", table);
                            self.service = table;
                        },
                        InternalMsg::Shutdown => {
                            adaptor.terminate();
                            warn!("The processor will shut down");
                        },
                    }
                },
                Some(msg) = pending_msgs.pull(), if !pending_msgs.is_empty() => {
                    debug!("Timeout message {:?}", msg);

                    // Return a timeout error message to the sender
                    let service_name = msg.get_service().clone();
                    msg.return_error_to_sender(None, prosa::core::service::ServiceError::Timeout(service_name, 200)).await.unwrap();
                },
            }
        }
    }
}
```

## Regulator - `Regulator`

The [`Regulator`](https://docs.rs/prosa/latest/prosa/event/speed/struct.Regulator.html) is used to regulate the flow of transaction to avoid overwhelming a remote peer.
It can be useful if you have a contract with a maximum number of parallel transactions, or limitations on transactions per second.

It serves two main goals:
- Enforce a threshold on transaction flow
- Limit a fixed number of outstanding transactions

All parameters for the regulator are defined in the [`new()`](https://docs.rs/prosa/latest/prosa/event/speed/struct.Regulator.html#method.new) method.

Using the object is pretty simple:
- When you send a transaction, call [`notify_send_transaction()`](https://docs.rs/prosa/latest/prosa/event/speed/struct.Regulator.html#method.notify_send_transaction). This may block your send if you exceed your allowed rate.
- When you receive a transaction, call [`notify_receive_transaction()`](https://docs.rs/prosa/latest/prosa/event/speed/struct.Regulator.html#method.notify_receive_transaction), which signals possible overload at the remote, and helps prevent too many concurrent transactions.

To check if you can send the next transaction, call [`tick()`](https://docs.rs/prosa/latest/prosa/event/speed/struct.Regulator.html#method.tick).
This method blocks if you need to wait, and lets you continue if you are within the allowed threshold.
