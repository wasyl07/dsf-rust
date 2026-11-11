# dsf-rust

A synchronous, blocking Rust client for the [Duet3D DuetSoftwareFramework](https://github.com/Duet3D/DuetSoftwareFramework)
(DSF) UNIX socket API. It is a port of the official `dsf-python` library, using only
`std::os::unix::net` (no async runtime).

Supported: command connections, object model subscriptions (full and patch), code interception
and custom HTTP endpoints.

The object model is **untyped**: it is a plain `serde_json::Value`. Use
`dsf::object_model::apply_patch` to merge patches received in patch subscription mode.

## Usage

```rust
use dsf::{CodeChannel, CommandApi, CommandConnection};

fn main() -> dsf::Result<()> {
    let mut conn = CommandConnection::new();
    conn.connect_default()?; // /run/dsf/dcs.sock or per /opt/dsf/conf/config.json
    let reply = conn.perform_simple_code("M115", CodeChannel::Sbc, false)?;
    println!("{reply}");
    Ok(())
}
```

Intercepting codes (`InterceptConnection`) blocks in `receive_code()`; call
`connection.canceller()` to get a handle that can make it return `Error::Cancelled` from another
thread. See `examples/` for subscribing, intercepting and HTTP endpoints.

Receive timeouts: command connections default to 3 seconds; intercept and subscribe
connections wait indefinitely by default.

## Non-blocking codes with a callback

`CommandConnection::perform_simple_code_cb` returns immediately and calls your closure on a
background worker thread (with its own connection and no receive timeout) once the code is done.
Jobs run in submission order, but only relative to other `_cb` calls.

```rust
conn.perform_simple_code_cb("G28", CodeChannel::Sbc, |r| println!("G28 done: {r:?}"))?;
// ... do other work ...
conn.wait_pending(); // wait for callbacks before exiting
```

## Async (tokio)

Enable the optional `tokio` feature (not default; no extra dependencies without it):

```toml
dsf = { version = "0.1", features = ["tokio"] }
```

`dsf::asynchronous` mirrors the blocking API one-to-one (`AsyncCommandConnection`,
`AsyncSubscribeConnection`, `AsyncInterceptConnection`, `AsyncCommandApi`,
`AsyncHttpEndpointUnixSocket`). All futures are `Send`, so they work inside `tokio::spawn`.

```rust
use dsf::asynchronous::{AsyncCommandApi, AsyncCommandConnection};
use dsf::CodeChannel;

#[tokio::main]
async fn main() -> dsf::Result<()> {
    let mut conn = AsyncCommandConnection::new();
    conn.connect_default().await?;
    println!("{}", conn.perform_simple_code("M115", CodeChannel::Sbc, false).await?);
    Ok(())
}
```

Receiving (`receive_code`, `get_object_model`, ...) is cancel-safe: race it with `tokio::select!`
or `tokio::time::timeout` without losing data. Sending and command calls are not cancel-safe.
There is no canceller object on the async side; use `select!`:

```rust
tokio::select! {
    code = conn.receive_code() => { /* handle code? */ }
    _ = tokio::signal::ctrl_c() => { /* shut down */ }
}
```

## License

AGPL-3.0-only, see `LICENSE`.
