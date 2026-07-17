# APN

An **APN** (Application Programming Node) lets a processor react to a service request by running a small *automaton* that can call other services, branch on their results, and produce the final response — without hand-writing a state machine over the processor loop.

An APN associates a service request with an automaton that handles it. Your code implements the automaton, while the framework takes care of running it and returning the response. ProSA exposes this as a lightweight primitive rather than a dedicated processor: you launch an APN directly on a request from within any processor.

## When to use an APN

Use an APN when handling a request means *making one or more sub-calls and deciding what to do next based on their responses*. For example: call an authorization service, and depending on its return code, call either a payment service or a rejection service, then return the outcome to the original caller.

Without an APN you would have to store the in-flight request in a [`PendingMsgs`](./ch03-06-events.md) map, send the sub-call yourself, match the response on a later loop iteration, correlate it back, and repeat for every step. An APN collapses all of that into a single linear (or branching) block of `async` code.

## Limitations

An APN has a few limitations by design:

- **It only processes service requests.** It cannot drive a timer, open a socket, or manage any external resource.
- **The automaton runs under a timeout budget.** An APN must never block for long; keep the budget short.

The automaton runs on its own spawned task, so it does **not** block the processor loop — but because it is spawned it must be `Send + 'static`: it captures owned data only and cannot borrow the processor's state.

If your need matches any of these limitations, write a full ProSA [processor](./ch03-00-proc.md) instead.

## Usage

An APN is launched with [`RequestMsg::apn`](https://docs.rs/prosa/latest/prosa/core/msg/struct.RequestMsg.html#method.apn), called directly on the request you want to handle. It takes a service table snapshot and a timeout that is the **overall budget for the whole automaton**, spawns a Tokio task for the automaton, and returns straight away — it is a plain (non-`async`) call.

The APN hands the automaton closure the [`Apn`](https://docs.rs/prosa/latest/prosa/core/apn/struct.Apn.html) handle plus the request's **service name and data** (a `String` and the `M`). There is no automatic first call: the automaton drives every sub-call itself with `apn.call(...)`, branches on the results, and returns the final `M`. That result is **sent back** to the original requestor on the request's response queue — you never call `return_to_sender` yourself. If the automaton needs the request's trace span (to nest its own spans), it's available via [`apn.trace_id()`](https://docs.rs/prosa/latest/prosa/core/apn/struct.Apn.html#method.trace_id).

Build a [`RequestMsg`](https://docs.rs/prosa/latest/prosa/core/msg/struct.RequestMsg.html) with its **response queue** set to your processor's own service queue ([`get_service_queue()`](https://docs.rs/prosa/latest/prosa/core/proc/struct.ProcParam.html#method.get_service_queue)) — that queue is where the APN's final result lands — then call [`apn`](https://docs.rs/prosa/latest/prosa/core/msg/struct.RequestMsg.html#method.apn) on it. Since the automaton is spawned, everything it needs must be captured by value:

```rust,noplayground
// `trans` is the request to handle (its response queue points back at this processor).
trans.apn(
    self.service.clone(),
    self.settings.apn_timeout,
    move |apn, _service, data| async move {
        // `data` is the request payload; drive the sub-calls from it.
        let mut auth = apn.call("AUTH", data).await?;
        let auth_data = auth.take_data().unwrap_or_default();
        let mut resp = match auth_data.get_unsigned(1).unwrap_or(0) {
            0 => apn.call("PAY", auth_data).await?,      // final response, auto-sent
            _ => apn.call("REJECT", auth_data).await?,
        };
        Ok(resp.take_data().unwrap_or_default())
    },
);
```

The automaton can create any object it needs; just remember it captures owned values (clone what you need out of the processor before launching).

### Sub-calls

The [`Apn`](https://docs.rs/prosa/latest/prosa/core/apn/struct.Apn.html) handle exposes two methods:

- [`call()`](https://docs.rs/prosa/latest/prosa/core/apn/struct.Apn.html#method.call) — sub-call a service; not individually timed out, it is bounded only by the APN's overall timeout budget.
- [`call_with_timeout()`](https://docs.rs/prosa/latest/prosa/core/apn/struct.Apn.html#method.call_with_timeout) — sub-call with an explicit timeout for that one call.

Both return `Result<ResponseMsg<M>, ServiceError>` — take the data out of the response to use it — so failures are handled with ordinary `?` / `match`:

- `ServiceError::UnableToReachService` — the service isn't in the table, or the send failed.
- `ServiceError::Timeout` — the service didn't respond within the timeout.
- Any error returned by the sub-called service is forwarded as-is.

Each sub-call gets its own dedicated response channel, so a reply can never be mistaken for another call's. Sub-call traces are nested under the original request's span, so a full APN flow shows up as a single trace tree.

If a sub-call fails (unreachable or timeout), propagate it with `?` and the error is returned straight to the original caller.

### Parallel sub-calls

Because [`call()`](https://docs.rs/prosa/latest/prosa/core/apn/struct.Apn.html#method.call) borrows `&self`, an automaton can fan out to several distinct services at once and await them together with [`tokio::join!`](https://docs.rs/tokio/latest/tokio/macro.join.html) — each sub-call has its own response channel, so their replies never interfere:

```rust,noplayground
move |apn, _service, data| async move {
    // Fire both sub-calls, then await both.
    let (pay, fraud) = tokio::join!(
        apn.call("PAY", data.clone()),
        apn.call("FRAUD", data),
    );
    let mut pay = pay?;
    let _fraud = fraud?;
    Ok(pay.take_data().unwrap_or_default())
}
```

## States

The "each state is a separate implementation" model maps naturally onto plain control flow: the automaton *is* the closure, and it holds its own state. A multi-state machine is just a loop over an enum, where each arm may issue a sub-call and transition to the next state. The request data seeds the initial state:

```rust,noplayground
enum State {
    Start(M),
    Authorized(M),
    Paid(M),
}

trans.apn(self.service.clone(), timeout, move |apn, _service, data| async move {
    let mut state = State::Start(data);
    loop {
        state = match state {
            State::Start(s) => {
                State::Authorized(apn.call("AUTH", s).await?.take_data().unwrap_or_default())
            }
            State::Authorized(a) if a.get_unsigned(1).unwrap_or(0) == 0 => {
                State::Paid(apn.call("PAY", a).await?.take_data().unwrap_or_default())
            }
            State::Authorized(a) => {
                return Ok(apn.call("REJECT", a).await?.take_data().unwrap_or_default());
            }
            State::Paid(p) => return Ok(p),
        };
    }
});
```

## Relation to `PendingMsgs`

An APN and [`PendingMsgs`](./ch03-06-events.md) solve related problems from opposite ends:

- Use an **APN** when the follow-up logic is a self-contained linear or branching flow that can run on its own task from captured data.
- Use **`PendingMsgs`** when you need the processor loop itself to keep driving each response and timeout across loop iterations — for instance to share and mutate processor state per response.
