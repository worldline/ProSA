# I/O & TVF Reference

Patterns for network I/O (listeners, streams) and TVF message creation.

## StreamListener — Server Sockets

Bind and accept connections. Supports UNIX, TCP, and SSL sockets.

```rust
use prosa::io::listener::{ListenerSetting, StreamListener};

// From configuration (YAML):
// listener:
//   url: "0.0.0.0:8080"
//   ssl:
//     cert: "/opt/cert.pem"
//     key: "/opt/cert.key"
//   max_socket: 4000000

// Bind from settings
let listener = self.settings.listener.bind().await?;

// Or bind manually
let listener = StreamListener::bind("0.0.0.0:8080").await?;

// Accept connections in a loop
loop {
    tokio::select! {
        Ok((stream, addr)) = listener.accept() => {
            // stream: Stream, addr: Option<SocketAddr>
            // Handle connection in a spawned task
            tokio::spawn(async move {
                handle_connection(stream, addr).await;
            });
        }
        // ... other select branches
    }
}
```

### Raw accept (for manual SSL handshake)

```rust
let (raw_stream, addr) = listener.accept_raw().await?;
// Perform handshake later when ready
let ssl_stream = listener.handshake(raw_stream).await?;
```

## Stream — Client Sockets

Connect to remote servers. Supports UNIX, TCP, SSL, and HTTP proxy variants.

```rust
use prosa::io::stream::{Stream, TargetSetting};

// From configuration (YAML):
// stream:
//   url: "example.com:443"
//   ssl:
//     store:
//       path: /etc/ssl/certs/
//   proxy: "http://myproxy:3128"
//   connect_timeout: 3000

// Connect from settings
let stream = self.settings.target.connect().await?;

// Or connect manually
let stream = Stream::connect_tcp("example.com:443").await?;
let stream = Stream::connect_openssl("example.com", 443, &ssl_connector).await?;
let stream = Stream::connect_tcp_with_http_proxy("proxy:3128", "target:443").await?;

// Socket options
stream.set_nodelay(true)?;
stream.set_ttl(64)?;

// Check protocol
if stream.is_ssl() {
    // SSL-specific handling
}
```

## The `#[io]` Macro

Creates I/O handler structs with automatic stream management fields.

```rust
use prosa::core::proc::io;
use bytes::BytesMut;

#[io]
pub struct MyProtocolHandler {
    // Your custom fields here
    pub session_id: u64,
}
```

The macro automatically adds:
- `stream: IO` — the network stream (generic: `AsyncReadExt + AsyncWriteExt + Unpin + Send`)
- `addr: Option<SocketAddr>` — remote address
- `buffer: BytesMut` — 16KB read buffer
- `socket_id: u32` — derived from raw file descriptor
- `From<IO>` and `From<(IO, SocketAddr)>` trait implementations

Usage after accept:
```rust
let (stream, addr) = listener.accept().await?;
let handler = MyProtocolHandler::from((stream, addr));
// handler.stream, handler.addr, handler.buffer, handler.socket_id are available
```

## TVF Messages — `tvf!` Macro

Construct TVF messages with a concise syntax instead of manual `put_*` calls.

### Map syntax (key-value pairs)

```rust
use prosa_macros::tvf;
use prosa_utils::msg::simple_string_tvf::SimpleStringTvf;

let msg = tvf!(SimpleStringTvf {
    1 => "hello",
    2 => 42,           // signed (i64) by default
    3 => 3.14,         // float
});
```

### Type annotations

Use `as Type` to specify the field type:

```rust
let msg = tvf!(SimpleStringTvf {
    1 => 100 as Unsigned,              // u64
    2 => -5 as Signed,                 // i64
    3 => 2.5 as Float,                 // f64
    4 => 0xFF as Byte,                 // u8
    5 => 0x01020304 as Bytes,          // Bytes
    6 => "1995-01-10" as Date,         // NaiveDate
    7 => "2023-06-05 15:02:00.000" as DateTime, // NaiveDateTime
});
```

### List syntax (sequential indices starting at 1)

```rust
let list = tvf!(SimpleStringTvf [
    "first",    // index 1
    "second",   // index 2
    42,         // index 3
]);
```

### Nested structures

```rust
let msg = tvf!(SimpleStringTvf {
    1 => "header",
    2 => [                    // nested list at key 2
        1 as Unsigned,
        "nested_value",
        {                     // nested map inside the list
            1 => "deep",
            2 => 0x0102 as Bytes,
        }
    ],
    3 => "2023-06-05 15:02:00.000" as DateTime,
});
```

### Manual TVF operations

```rust
use prosa::core::msg::Tvf;

let mut msg = SimpleStringTvf::default();

// Put values
msg.put_string(1, "value");
msg.put_unsigned(2, 42u64);
msg.put_signed(3, -10i64);
msg.put_float(4, 3.14f64);
msg.put_byte(5, 0xFFu8);

// Get values (returns Result, never unwrap!)
let s = msg.get_string(1)?;     // Cow<'_, String>
let n = msg.get_unsigned(2)?;   // u64
let i = msg.get_signed(3)?;     // i64
let f = msg.get_float(4)?;      // f64

// Container operations
msg.contains(1);  // true
msg.len();        // number of fields
msg.is_empty();   // false
msg.remove(1);    // remove field
msg.keys();       // iterate over field IDs
```

## Declare a Custom TVF

If you implement your own TVF type:

```rust
impl Tvf for MyOwnTvf {
    // Implement all trait methods
}
```

Ensure you also implement: `Default`, `Send`, `Sync`, `Clone`, `Debug`.

Declare in Cargo.toml:
```toml
[package.metadata.prosa]
tvf = ["tvf::MyOwnTvf"]
```

## TvfFilter — Masking Sensitive Data

Implement `TvfFilter` to mask fields like PAN, CVV before logging:

```rust
use prosa_utils::msg::tvf::TvfFilter;

impl TvfFilter for MyTvf {
    fn filter<T: Tvf>(mut buf: T) -> T {
        // Mask sensitive fields before logging
        if buf.contains(10) {
            buf.put_string(10, "****");
        }
        buf
    }
}
```
