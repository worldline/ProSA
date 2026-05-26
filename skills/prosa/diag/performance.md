# Performance Diagnostics Reference

## PendingMsgs — Timeout Tracking

Track in-flight requests and detect timeouts. Essential for production processors.

### Tuning timeout values

```rust
// Short timeout for real-time services
pending_msgs.push(msg, Duration::from_millis(100));

// Longer timeout for batch processing
pending_msgs.push(msg, Duration::from_secs(30));
```

### Diagnosing timeout issues

If you see frequent timeouts:
1. Check downstream service health (processor state in Grafana)
2. Check if the service queue is full (too many pending requests)
3. Increase timeout if the downstream legitimately needs more time
4. Add more processor instances for the target service (horizontal scaling)

### Timeout error propagation

When a timeout fires, return the error to the caller with overhead info:

```rust
Some(timed_out_msg) = pending_msgs.pull(), if !pending_msgs.is_empty() => {
    let service_name = timed_out_msg.get_service().clone();
    let _ = timed_out_msg.return_error_to_sender(
        None,
        ServiceError::Timeout(service_name, timeout_ms),
    ).await;
}
```

The `overhead` value in `Timeout(service, overhead)` is used by the Regulator to slow down sending.

## Regulator — Flow Control

Rate-limit outgoing transactions when the remote system has TPS limits or max concurrent connections.

### Parameters

```rust
let regulator = Regulator::new(
    100.0,                      // max_speed: TPS limit
    Duration::from_secs(10),    // timeout_threshold: slow down if response > 10s
    10,                         // max_concurrents_send: parallel request limit
    15,                         // speed_interval: samples for speed calculation
);
```

### Tuning guidelines

| Symptom | Adjustment |
|---------|------------|
| Downstream overloaded | Lower `max_speed` |
| Responses slow under load | Lower `max_concurrents_send` |
| Throughput too low | Increase `max_speed` and `max_concurrents_send` |
| Speed oscillates | Increase `speed_interval` for smoother averaging |
| Timeouts cause cascading slowdown | Check `timeout_threshold` vs actual latency |

### Flow control loop

```rust
// On response: update speed calculation
regulator.notify_receive_transaction(msg.elapsed());

// On timeout error: add overhead to slow down
if let ServiceError::Timeout(_, overhead) = err.get_err() {
    regulator.add_tick_overhead(Duration::from_millis(*overhead));
}
regulator.notify_receive_transaction(err.elapsed());

// tick() blocks until next send is allowed
_ = regulator.tick(), if self.service.exist_proc_service("SERVICE") => {
    // Send transaction
    regulator.notify_send_transaction();
}
```

## Queue Diagnostics

ProSA provides queue implementations in `prosa_utils::queue`:

| Queue | Type | Use case |
|-------|------|----------|
| `mpsc` | Multi-producer, single-consumer | Standard processor queues |
| `spmc` | Single-producer, multi-consumer | Fan-out patterns |
| `lockfree` | Atomic/lock-free | High-throughput, low-latency |

### Queue health checks

```rust
use prosa_utils::queue::QueueChecker;

queue.is_empty()       // no items pending
queue.is_full()        // at capacity — sends will block/fail
queue.len()            // current item count
queue.max_capacity()   // maximum items
```

### Queue errors

- `QueueError::Empty` — consumer tried to read from empty queue
- `QueueError::Full(item, size)` — queue at max capacity, item rejected
- `QueueError::Retrieve(index)` — internal retrieval error

If queues are frequently full, consider:
1. Increasing queue capacity (channel buffer size)
2. Adding more consumer processors
3. Reducing producer send rate via Regulator

## Worker Thread Tuning

```bash
# Main task threads (default: 1)
prosa_binary -t 4 -c config.yaml
```

Each processor runs in its own thread(s). The `-t` flag only controls the Main task's Tokio runtime. For processor-level threading, see processor-specific documentation.

## Scaling Patterns

### Horizontal: multiple processor instances

Register multiple instances of the same processor on the same service. `ServiceTable` round-robins across them automatically.

### Vertical: subtasks within a processor

Use the multi-subtask pattern to spawn concurrent workers within a single processor, each with its own queue and service subscriptions. See `dev/events-services.md` for the pattern.
