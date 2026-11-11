//! Error types.

use std::io;

/// Errors returned by this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An I/O error on the socket.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    /// A JSON (de)serialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    /// The server speaks an older protocol version than this client.
    #[error("incompatible API version (need {need}, got {got})")]
    IncompatibleVersion {
        /// Version required by this client.
        need: u32,
        /// Version reported by the server.
        got: u32,
    },
    /// The server rejected the connection mode.
    #[error("connection rejected ({error_type}: {error_message})")]
    ConnectionRejected {
        /// Error type reported by the server.
        error_type: String,
        /// Error message reported by the server.
        error_message: String,
    },
    /// The server failed to process a command.
    #[error("command {command} failed ({error_type}: {error_message})")]
    InternalServer {
        /// Name of the command.
        command: String,
        /// Error type reported by the server.
        error_type: String,
        /// Error message reported by the server.
        error_message: String,
    },
    /// The command was cancelled on the server (`TaskCanceledException`).
    #[error("task cancelled: {0}")]
    TaskCanceled(String),
    /// Timed out while waiting for data from the server.
    #[error("timeout while waiting for JSON response")]
    Timeout,
    /// The receive was cancelled via an [`InterceptCanceller`](crate::InterceptCanceller).
    #[error("receive cancelled")]
    Cancelled,
    /// The peer closed the connection.
    #[error("connection closed")]
    ConnectionClosed,
    /// The connection has not been established (or was closed).
    #[error("not connected")]
    NotConnected,
    /// A code or parameter could not be parsed or converted.
    #[error("code parser error: {0}")]
    CodeParser(String),
    /// An invalid argument was supplied.
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
}

/// Result alias using [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
