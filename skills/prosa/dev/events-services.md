# Events & Services Reference

Patterns for timeout tracking, flow control, service registration, and multi-subtask processors.

## PendingMsgs — Timeout Tracking

Track in-flight messages and handle timeouts. Essential for production processors that send requests to other services.

```rust
use prosa::event::pending::PendingMsgs;
use prosa::core::msg::{InternalMsg, Msg, RequestMsg};
use std::time::Duration;

// Inside internal_run():
let mut pending_msgs: PendingMsgs<RequestMsg<M>, M> = Default::default();

loop {
    tokio::select! {
        Some(msg) = self.internal_rx_queue.recv() => {
            match msg {
                InternalMsg::Request(msg) => {
                    // Track the request with a timeout
                    pending_msgs.push(msg, Duration::from_millis(500));
                }
                InternalMsg::Response(msg) => {
                    let _enter = msg.enter_span();
                    // Retrieve the original request (removes from tracking)
                    // Returns None if already timed out
                    if let Some(original_request) = pending_msgs.pull_msg(msg.get_id()) {
                        // Process response with original request context
                    }
                }
                InternalMsg::Error(err) => {
                    let _enter = err.enter_span();
                    // Also remove from pending on error
                    let _ = pending_msgs.pull_msg(err.get_id());
                }
                InternalMsg::Service(table) => self.service = table,
                InternalMsg::Shutdown => {
                    adaptor.terminate();
                    self.proc.remove_proc(None).await?;
                    return Ok(());
                }
                _ => {}
            }
        }
        // IMPORTANT: the `if !pending_msgs.is_empty()` guard prevents
        // the select branch from being polled when there are no pending messages
        Some(timed_out_msg) = pending_msgs.pull(), if !pending_msgs.is_empty() => {
            // Return timeout error to sender
            let service_name = timed_out_msg.get_service().clone();
            let _ = timed_out_msg.return_error_to_sender(
                None,
                ServiceError::Timeout(service_name, 500)
            ).await;
        }
    }
}
```

## Regulator — Flow Control

Rate-limit outgoing transactions. Useful when the remote system has TPS limits or max concurrent connections.

```rust
use prosa::event::speed::Regulator;
use std::time::Duration;

// Create a regulator:
//   max_speed: 100.0 TPS
//   timeout_threshold: slow down if response takes > 10s
//   max_concurrents_send: max 10 parallel requests
//   speed_interval: 15 samples for speed calculation
let mut regulator = Regulator::new(100.0, Duration::from_secs(10), 10, 15);

// Or from InjSettings (built-in helper):
// let mut regulator = self.settings.get_regulator();

loop {
    tokio::select! {
        Some(msg) = self.internal_rx_queue.recv() => {
            match msg {
                InternalMsg::Response(msg) => {
                    // Signal that a response was received (updates flow control)
                    regulator.notify_receive_transaction(msg.elapsed());
                }
                InternalMsg::Error(err) => {
                    // On timeout errors, add overhead to slow down
                    if let ServiceError::Timeout(_, overhead) = err.get_err() {
                        regulator.add_tick_overhead(Duration::from_millis(*overhead));
                    }
                    regulator.notify_receive_transaction(err.elapsed());
                }
                // ... handle other variants
                _ => {}
            }
        }
        // tick() blocks until we're allowed to send the next transaction
        _ = regulator.tick(), if self.service.exist_proc_service("SERVICE_NAME") => {
            if let Some(service) = self.service.get_proc_service("SERVICE_NAME") {
                let trans = RequestMsg::new(
                    "SERVICE_NAME".to_string(),
                    M::default(), // build your transaction
                    self.proc.get_service_queue(),
                );
                let _ = service.proc_queue.send(InternalMsg::Request(trans)).await;
                regulator.notify_send_transaction();
            }
        }
    }
}
```

## Single Service Pattern

The most common pattern: register services and handle incoming requests.

```rust
#[proc]
impl<A> Proc<A> for MyProc
where
    A: Adaptor + std::marker::Send + std::marker::Sync,
{
    async fn internal_run(&mut self) -> Result<(), Box<dyn ProcError + Send + Sync>> {
        let adaptor = A::new(self)?;
        self.proc.add_proc().await?;
        self.proc
            .add_service_proc(vec!["MY_SERVICE".to_string()])
            .await?;

        loop {
            if let Some(msg) = self.internal_rx_queue.recv().await {
                match msg {
                    InternalMsg::Request(mut msg) => {
                        if let Ok(data) = msg.get_data() {
                            let response = data.clone(); // process data
                            let _ = msg.return_result_to_sender(response);
                        }
                    }
                    InternalMsg::Response(_) => {}
                    InternalMsg::Error(_) => {}
                    InternalMsg::Command(_) => {}
                    InternalMsg::Config => {}
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

## Sender-Only Pattern

For processors that only send requests (no services to listen on).

```rust
// Register to receive responses (required even if not listening to services)
self.proc.add_proc().await?;

// Wait for the service table before sending
loop {
    if let Some(msg) = self.internal_rx_queue.recv().await {
        match msg {
            InternalMsg::Response(msg) => { /* handle response */ }
            InternalMsg::Error(err) => { /* handle error */ }
            InternalMsg::Service(table) => self.service = table,
            InternalMsg::Shutdown => {
                self.proc.remove_proc(None).await?;
                return Ok(());
            }
            _ => {}
        }
    }

    // Send when the target service is available
    if let Some(service) = self.service.get_proc_service("TARGET_SERVICE") {
        let trans = RequestMsg::new(
            "TARGET_SERVICE".to_string(),
            M::default(),
            self.proc.get_service_queue(),
        );
        let _ = service.proc_queue.send(InternalMsg::Request(trans)).await;
    }
}
```

## Multi-Subtask Pattern

Spawn subtasks with their own message queues and service subscriptions.

```rust
async fn internal_run(&mut self) -> Result<(), Box<dyn ProcError + Send + Sync>> {
    self.proc.add_proc().await?;

    // Create a subtask with its own queue
    let (tx_queue, mut rx_queue) = tokio::sync::mpsc::channel(2048);
    let sub_proc = self.proc.clone();
    let subtask_id = 1;

    tokio::spawn(async move {
        // Register the subtask queue with the bus
        if let Err(e) = sub_proc
            .add_proc_queue(tx_queue.clone(), subtask_id)
            .await
        {
            return;
        }

        // Register services specific to this subtask
        let _ = sub_proc
            .add_service(vec!["SUBTASK_SERVICE".to_string()], subtask_id)
            .await;

        let mut service = std::sync::Arc::new(prosa::core::service::ServiceTable::default());

        loop {
            if let Some(msg) = rx_queue.recv().await {
                match msg {
                    InternalMsg::Request(msg) => {
                        // Handle requests for this subtask
                    }
                    InternalMsg::Service(table) => service = table,
                    InternalMsg::Shutdown => {
                        // Cleanup subtask
                        return;
                    }
                    _ => {}
                }
            }
        }
    });

    // Main task handles its own services
    self.proc
        .add_service_proc(vec!["MAIN_SERVICE".to_string()])
        .await?;

    loop {
        if let Some(msg) = self.internal_rx_queue.recv().await {
            match msg {
                InternalMsg::Request(msg) => { /* main task requests */ }
                InternalMsg::Service(table) => self.service = table,
                InternalMsg::Shutdown => {
                    self.proc.remove_proc(None).await?;
                    return Ok(());
                }
                _ => {}
            }
        }
    }
}
```

### Multi-subtask sending

Each subtask that sends requests must use its own queue for receiving responses:

```rust
let (tx_queue, mut rx_queue) = tokio::sync::mpsc::channel(2048);
let tx_msg_queue = tx_queue.clone(); // clone for sending
let sub_proc = self.proc.clone();
let subtask_id = 1;

tokio::spawn(async move {
    sub_proc.add_proc_queue(tx_queue, subtask_id).await?;

    let mut service = std::sync::Arc::new(prosa::core::service::ServiceTable::default());

    loop {
        if let Some(msg) = rx_queue.recv().await {
            match msg {
                InternalMsg::Response(msg) => { /* handle response for this subtask */ }
                InternalMsg::Service(table) => service = table,
                _ => {}
            }
        }

        // Send using this subtask's queue as response channel
        if let Some(svc) = service.get_proc_service("TARGET") {
            let trans = RequestMsg::new(
                "TARGET".to_string(),
                M::default(),
                tx_msg_queue.clone(),
            );
            let _ = svc.proc_queue.send(InternalMsg::Request(trans)).await;
        }
    }
});
```
