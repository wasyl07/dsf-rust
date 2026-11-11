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

## License

AGPL-3.0-only, see `LICENSE`.
