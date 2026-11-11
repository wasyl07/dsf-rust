#![cfg_attr(docsrs, feature(doc_cfg))]
//! Synchronous client for the Duet3D DuetSoftwareFramework (DSF) UNIX socket API.
//!
//! The object model is not typed: it is a plain [`serde_json::Value`]. Use
//! [`object_model::apply_patch`] to apply patches received in patch subscription mode.
//!
//! With the optional `tokio` cargo feature, an async API is available in the `asynchronous` module.
//!
//! ```no_run
//! use dsf::{CodeChannel, CommandApi, CommandConnection};
//!
//! # fn main() -> dsf::Result<()> {
//! let mut conn = CommandConnection::new();
//! conn.connect_default()?;
//! let reply = conn.perform_simple_code("M115", CodeChannel::Sbc, false)?;
//! println!("{reply}");
//! # Ok(()) }
//! ```

#[cfg(feature = "tokio")]
#[cfg_attr(docsrs, doc(cfg(feature = "tokio")))]
pub mod asynchronous;
pub mod code;
pub mod code_parameter;
pub mod commands;
pub mod connection;
pub mod error;
mod framing;
pub mod http;
pub mod object_model;
pub mod types;

pub use code::Code;
pub use code_parameter::{CodeParameter, ParameterValue};
pub use commands::{Command, InitMessage};
pub use connection::{
    BaseConnection, CommandApi, CommandConnection, InterceptCanceller, InterceptConnection,
    InterceptOptions, Response, SubscribeConnection,
};
pub use error::{Error, Result};
pub use http::{
    HttpEndpointConnection, HttpEndpointUnixSocket, HttpResponseType, ReceivedHttpRequest,
};
pub use types::*;

use std::path::PathBuf;

/// DSF protocol version implemented by this crate.
pub const PROTOCOL_VERSION: u32 = 12;
/// Default backlog for HTTP endpoint sockets.
pub const DEFAULT_BACKLOG: u32 = 4;
/// Default DSF control server socket.
pub const DEFAULT_SOCKET_FILE: &str = "/run/dsf/dcs.sock";
/// DSF configuration file consulted by [`default_socket_file`].
pub const CONFIG_PATH: &str = "/opt/dsf/conf/config.json";

/// Compute the socket path from the contents of the DSF `config.json`.
///
/// Uses `SocketDirectory` (default `/run/dsf`) and `SocketFile` (default `dcs.sock`);
/// invalid JSON or wrongly typed keys fall back to the defaults.
pub fn socket_file_from_config(config_json: &str) -> PathBuf {
    let v: serde_json::Value = serde_json::from_str(config_json).unwrap_or_default();
    let dir = v
        .get("SocketDirectory")
        .and_then(|d| d.as_str())
        .unwrap_or("/run/dsf");
    let file = v
        .get("SocketFile")
        .and_then(|d| d.as_str())
        .unwrap_or("dcs.sock");
    PathBuf::from(dir).join(file)
}

/// The default socket file: from [`CONFIG_PATH`] if readable, else [`DEFAULT_SOCKET_FILE`].
pub fn default_socket_file() -> PathBuf {
    match std::fs::read_to_string(CONFIG_PATH) {
        Ok(s) => socket_file_from_config(&s),
        Err(_) => PathBuf::from(DEFAULT_SOCKET_FILE),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_parsing() {
        assert_eq!(
            socket_file_from_config("{}"),
            PathBuf::from("/run/dsf/dcs.sock")
        );
        assert_eq!(
            socket_file_from_config(r#"{"SocketDirectory":"/tmp/x","SocketFile":"a.sock"}"#),
            PathBuf::from("/tmp/x/a.sock")
        );
        assert_eq!(
            socket_file_from_config(r#"{"SocketDirectory":"/tmp/x"}"#),
            PathBuf::from("/tmp/x/dcs.sock")
        );
        assert_eq!(
            socket_file_from_config("not json"),
            PathBuf::from("/run/dsf/dcs.sock")
        );
        assert_eq!(
            socket_file_from_config(r#"{"SocketFile":5}"#),
            PathBuf::from("/run/dsf/dcs.sock")
        );
        assert_eq!(
            socket_file_from_config("[1]"),
            PathBuf::from("/run/dsf/dcs.sock")
        );
    }
}
