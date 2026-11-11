//! Async (tokio) API, enabled by the `tokio` cargo feature.
//!
//! Mirrors the blocking API one-to-one: same method names, arguments and return types.
//!
//! # Cancel safety
//!
//! All receive methods (`receive`, `receive_json`, `receive_code`, `get_object_model`'s receive,
//! ...) are cancel-safe: dropping the future (via `tokio::select!` or `tokio::time::timeout`)
//! loses no bytes, and the next call returns the complete frame. Sending is not cancel-safe;
//! do not cancel `send` or command calls midway.
//!
//! ```no_run
//! # use dsf::asynchronous::*;
//! # use dsf::*;
//! # async fn f() -> dsf::Result<()> {
//! let mut opts = InterceptOptions::new(InterceptionMode::Pre);
//! opts.filters = Some(vec!["M1234".into()]);
//! let mut conn = AsyncInterceptConnection::new(opts);
//! conn.connect_default().await?;
//! tokio::select! {
//!     code = conn.receive_code() => { let _code = code?; }
//!     _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => { /* give up */ }
//! }
//! # Ok(()) }
//! ```

mod api;
mod base;
mod http;

pub use api::AsyncCommandApi;
pub use base::AsyncBaseConnection;
pub use http::{AsyncHttpEndpointConnection, AsyncHttpEndpointUnixSocket};

use crate::code::Code;
use crate::commands::{Command, InitMessage};
use crate::connection::{subscribe_init, InterceptOptions, DEFAULT_COMMAND_TIMEOUT};
use crate::error::Result;
use crate::types::{MessageType, SubscriptionMode};
use crate::{default_socket_file, PROTOCOL_VERSION};
use serde_json::Value;
use std::path::Path;
use std::time::Duration;

/// Async connection for sending commands to the control server.
#[derive(Debug)]
pub struct AsyncCommandConnection {
    base: AsyncBaseConnection,
}

impl Default for AsyncCommandConnection {
    fn default() -> Self {
        Self::new()
    }
}

impl AsyncCommandConnection {
    /// Create an unconnected command connection (receive timeout: 3 seconds).
    pub fn new() -> Self {
        AsyncCommandConnection {
            base: AsyncBaseConnection::new(Some(DEFAULT_COMMAND_TIMEOUT), false),
        }
    }

    /// Set the receive timeout (`None` waits indefinitely).
    pub fn with_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.base.set_timeout(timeout);
        self
    }

    /// Print sent/received JSON to stderr.
    pub fn with_debug(mut self, debug: bool) -> Self {
        self.base.set_debug(debug);
        self
    }

    /// Connect to the given UNIX socket.
    pub async fn connect(&mut self, socket_file: impl AsRef<Path> + Send) -> Result<()> {
        self.base
            .connect(
                &InitMessage::Command {
                    version: PROTOCOL_VERSION,
                },
                socket_file,
            )
            .await
    }

    /// Connect to the default socket (see [`default_socket_file`]).
    pub async fn connect_default(&mut self) -> Result<()> {
        self.connect(default_socket_file()).await
    }

    /// Close the connection.
    pub fn close(&mut self) {
        self.base.close();
    }

    /// Access the underlying base connection.
    pub fn base(&self) -> &AsyncBaseConnection {
        &self.base
    }
}

impl AsyncCommandApi for AsyncCommandConnection {
    fn connection_mut(&mut self) -> &mut AsyncBaseConnection {
        &mut self.base
    }
}

/// Async connection for receiving object model updates.
///
/// The receive timeout defaults to `None` (wait indefinitely for the next update).
#[derive(Debug)]
pub struct AsyncSubscribeConnection {
    base: AsyncBaseConnection,
    mode: SubscriptionMode,
    filter: String,
    filters: Option<Vec<String>>,
}

impl AsyncSubscribeConnection {
    /// Create an unconnected subscribe connection.
    pub fn new(mode: SubscriptionMode, filters: Option<Vec<String>>) -> Self {
        AsyncSubscribeConnection {
            base: AsyncBaseConnection::new(None, false),
            mode,
            filter: String::new(),
            filters,
        }
    }

    /// Set the obsolete delimited filter string (prefer `filters`).
    pub fn with_filter_string(mut self, filter: impl Into<String>) -> Self {
        self.filter = filter.into();
        self
    }

    /// Set the receive timeout (`None` waits indefinitely).
    pub fn with_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.base.set_timeout(timeout);
        self
    }

    /// Print sent/received JSON to stderr.
    pub fn with_debug(mut self, debug: bool) -> Self {
        self.base.set_debug(debug);
        self
    }

    /// Connect to the given UNIX socket.
    pub async fn connect(&mut self, socket_file: impl AsRef<Path> + Send) -> Result<()> {
        let init = subscribe_init(self.mode, &self.filter, &self.filters);
        self.base.connect(&init, socket_file).await
    }

    /// Connect to the default socket (see [`default_socket_file`]).
    pub async fn connect_default(&mut self) -> Result<()> {
        self.connect(default_socket_file()).await
    }

    /// Close the connection.
    pub fn close(&mut self) {
        self.base.close();
    }

    /// Access the underlying base connection.
    pub fn base(&self) -> &AsyncBaseConnection {
        &self.base
    }

    /// Receive the full object model and acknowledge it. Call this first after connecting.
    ///
    /// The receive is cancel-safe, but once the model has been received the acknowledge is
    /// sent; do not cancel after that point.
    pub async fn get_object_model(&mut self) -> Result<Value> {
        let model = self.base.receive().await?;
        self.base.send(&Command::Acknowledge).await?;
        Ok(model)
    }

    /// Receive the object model as raw JSON text and acknowledge it.
    pub async fn get_serialized_object_model(&mut self) -> Result<String> {
        let json = self.base.receive_json().await?;
        self.base.send(&Command::Acknowledge).await?;
        Ok(json)
    }

    /// Receive an object model update as raw JSON text and acknowledge it.
    ///
    /// In [`SubscriptionMode::Patch`] these are patches to be applied with
    /// [`apply_patch`](crate::object_model::apply_patch).
    pub async fn get_object_model_patch(&mut self) -> Result<String> {
        self.get_serialized_object_model().await
    }
}

/// Async connection for intercepting G/M/T-codes.
#[derive(Debug)]
pub struct AsyncInterceptConnection {
    base: AsyncBaseConnection,
    options: InterceptOptions,
}

impl AsyncInterceptConnection {
    /// Create an unconnected intercept connection.
    pub fn new(options: InterceptOptions) -> Self {
        AsyncInterceptConnection {
            base: AsyncBaseConnection::new(options.timeout, options.debug),
            options,
        }
    }

    /// Connect to the given UNIX socket.
    pub async fn connect(&mut self, socket_file: impl AsRef<Path> + Send) -> Result<()> {
        let init = self.options.init_message();
        self.base.connect(&init, socket_file).await
    }

    /// Connect to the default socket (see [`default_socket_file`]).
    pub async fn connect_default(&mut self) -> Result<()> {
        self.connect(default_socket_file()).await
    }

    /// Close the connection.
    pub fn close(&mut self) {
        self.base.close();
    }

    /// Access the underlying base connection.
    pub fn base(&self) -> &AsyncBaseConnection {
        &self.base
    }

    /// Wait for a code to be intercepted and read it.
    ///
    /// This is cancel-safe: to cancel a pending receive, race it in `tokio::select!` or wrap it
    /// in `tokio::time::timeout`. Nothing already received is lost; the next call returns the
    /// complete code. The connection's overall timeout still applies.
    ///
    /// ```no_run
    /// # use dsf::asynchronous::AsyncInterceptConnection;
    /// # async fn f(conn: &mut AsyncInterceptConnection) -> dsf::Result<()> {
    /// tokio::select! {
    ///     code = conn.receive_code() => println!("{}", code?),
    ///     _ = tokio::signal::ctrl_c() => {}
    /// }
    /// # Ok(()) }
    /// ```
    pub async fn receive_code(&mut self) -> Result<Code> {
        self.base.receive().await
    }

    /// Instruct the server to cancel the last received code. No response is read.
    pub async fn cancel_code(&mut self) -> Result<()> {
        self.base.send(&Command::Cancel).await
    }

    /// Instruct the server to pass the last received code on unmodified. No response is read.
    pub async fn ignore_code(&mut self) -> Result<()> {
        self.base.send(&Command::Ignore).await
    }

    /// Resolve the last received code with the given message. No response is read.
    pub async fn resolve_code(
        &mut self,
        message_type: MessageType,
        content: Option<&str>,
    ) -> Result<()> {
        self.base
            .send(&Command::Resolve {
                message_type,
                content: content.map(String::from),
            })
            .await
    }
}

impl AsyncCommandApi for AsyncInterceptConnection {
    fn connection_mut(&mut self) -> &mut AsyncBaseConnection {
        &mut self.base
    }
}
