//! Connections to the DSF control server.

mod api;
mod base;

pub use api::CommandApi;
pub use base::{BaseConnection, Response};

use crate::code::Code;
use crate::commands::{Command, InitMessage};
use crate::error::Result;
use crate::types::{CodeChannel, InterceptionMode, MessageType, SubscriptionMode};
use crate::{default_socket_file, PROTOCOL_VERSION};
use serde_json::Value;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Default receive timeout of command connections.
pub const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(3);

/// A connection for sending commands to the control server.
#[derive(Debug)]
pub struct CommandConnection {
    base: BaseConnection,
}

impl Default for CommandConnection {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandConnection {
    /// Create an unconnected command connection (receive timeout: 3 seconds).
    pub fn new() -> Self {
        CommandConnection {
            base: BaseConnection::new(Some(DEFAULT_COMMAND_TIMEOUT), false),
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
    pub fn connect(&mut self, socket_file: impl AsRef<Path>) -> Result<()> {
        self.base.connect(
            &InitMessage::Command {
                version: PROTOCOL_VERSION,
            },
            socket_file,
        )
    }

    /// Connect to the default socket (see [`default_socket_file`]).
    pub fn connect_default(&mut self) -> Result<()> {
        self.connect(default_socket_file())
    }

    /// Close the connection.
    pub fn close(&mut self) {
        self.base.close();
    }

    /// Access the underlying base connection.
    pub fn base(&self) -> &BaseConnection {
        &self.base
    }
}

impl CommandApi for CommandConnection {
    fn connection_mut(&mut self) -> &mut BaseConnection {
        &mut self.base
    }
}

/// A connection for receiving object model updates.
///
/// The receive timeout defaults to `None`, i.e. receiving waits indefinitely for the next
/// update. Use [`with_timeout`](Self::with_timeout) to change this.
#[derive(Debug)]
pub struct SubscribeConnection {
    base: BaseConnection,
    mode: SubscriptionMode,
    filter: String,
    filters: Option<Vec<String>>,
}

impl SubscribeConnection {
    /// Create an unconnected subscribe connection.
    pub fn new(mode: SubscriptionMode, filters: Option<Vec<String>>) -> Self {
        SubscribeConnection {
            base: BaseConnection::new(None, false),
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
    pub fn connect(&mut self, socket_file: impl AsRef<Path>) -> Result<()> {
        let init = InitMessage::Subscribe {
            version: PROTOCOL_VERSION,
            subscription_mode: self.mode,
            filter: self.filter.clone(),
            filters: self.filters.clone(),
        };
        self.base.connect(&init, socket_file)
    }

    /// Connect to the default socket (see [`default_socket_file`]).
    pub fn connect_default(&mut self) -> Result<()> {
        self.connect(default_socket_file())
    }

    /// Close the connection.
    pub fn close(&mut self) {
        self.base.close();
    }

    /// Access the underlying base connection.
    pub fn base(&self) -> &BaseConnection {
        &self.base
    }

    /// Receive the full object model and acknowledge it. Call this first after connecting.
    pub fn get_object_model(&mut self) -> Result<Value> {
        let model = self.base.receive()?;
        self.base.send(&Command::Acknowledge)?;
        Ok(model)
    }

    /// Receive the object model as raw JSON text and acknowledge it.
    pub fn get_serialized_object_model(&mut self) -> Result<String> {
        let json = self.base.receive_json()?;
        self.base.send(&Command::Acknowledge)?;
        Ok(json)
    }

    /// Receive an object model update as raw JSON text and acknowledge it.
    ///
    /// In [`SubscriptionMode::Patch`] these are patches to be applied with
    /// [`apply_patch`](crate::object_model::apply_patch).
    pub fn get_object_model_patch(&mut self) -> Result<String> {
        self.get_serialized_object_model()
    }
}

/// Options of an [`InterceptConnection`].
#[derive(Debug, Clone, PartialEq)]
pub struct InterceptOptions {
    /// Interception mode.
    pub mode: InterceptionMode,
    /// Channels to intercept codes on (default: all channels).
    pub channels: Vec<CodeChannel>,
    /// Code filters such as `M1234`, `G1`, `Q0` for comments, `T*`.
    pub filters: Option<Vec<String>>,
    /// Flush the channel before notifying the client when a filter is set.
    pub auto_flush: bool,
    /// Evaluate expression parameters before sending the code over.
    pub auto_evaluate_expression: bool,
    /// Intercept priority codes instead of regular codes.
    pub priority_codes: bool,
    /// Receive timeout (default: none, wait indefinitely).
    pub timeout: Option<Duration>,
    /// Print sent/received JSON to stderr.
    pub debug: bool,
}

impl InterceptOptions {
    /// Default options for the given mode.
    pub fn new(mode: InterceptionMode) -> Self {
        InterceptOptions {
            mode,
            channels: CodeChannel::ALL.to_vec(),
            filters: None,
            auto_flush: true,
            auto_evaluate_expression: true,
            priority_codes: false,
            timeout: None,
            debug: false,
        }
    }
}

/// A connection for intercepting G/M/T-codes.
#[derive(Debug)]
pub struct InterceptConnection {
    base: BaseConnection,
    options: InterceptOptions,
    cancel: Arc<AtomicBool>,
}

/// Handle to cancel a blocked [`InterceptConnection::receive_code`] from another thread.
#[derive(Debug, Clone, Default)]
pub struct InterceptCanceller(Arc<AtomicBool>);

impl InterceptCanceller {
    /// Make the current or next `receive_code` call return [`Error::Cancelled`](crate::Error::Cancelled).
    ///
    /// The request is one-shot: the call that returns `Cancelled` resets it.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

impl InterceptConnection {
    /// Create an unconnected intercept connection.
    pub fn new(options: InterceptOptions) -> Self {
        InterceptConnection {
            base: BaseConnection::new(options.timeout, options.debug),
            options,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    /// A handle that can cancel `receive_code` from another thread.
    pub fn canceller(&self) -> InterceptCanceller {
        InterceptCanceller(Arc::clone(&self.cancel))
    }

    /// Connect to the given UNIX socket.
    pub fn connect(&mut self, socket_file: impl AsRef<Path>) -> Result<()> {
        let o = &self.options;
        let init = InitMessage::Intercept {
            version: PROTOCOL_VERSION,
            interception_mode: o.mode,
            channels: o.channels.clone(),
            filters: o.filters.clone(),
            priority_codes: o.priority_codes,
            auto_flush: o.auto_flush,
            auto_evaluate_expressions: o.auto_evaluate_expression,
        };
        self.base.connect(&init, socket_file)
    }

    /// Connect to the default socket (see [`default_socket_file`]).
    pub fn connect_default(&mut self) -> Result<()> {
        self.connect(default_socket_file())
    }

    /// Close the connection.
    pub fn close(&mut self) {
        self.base.close();
    }

    /// Access the underlying base connection.
    pub fn base(&self) -> &BaseConnection {
        &self.base
    }

    /// Wait for a code to be intercepted and read it.
    ///
    /// Returns [`Error::Cancelled`](crate::Error::Cancelled) if an [`InterceptCanceller`] fires
    /// while waiting, or fired before this call. A complete code that is already buffered is
    /// returned before the cancel flag is checked. The connection stays usable after a
    /// cancellation and partially received data is kept. The overall timeout still applies.
    pub fn receive_code(&mut self) -> Result<Code> {
        let json = self.base.receive_json_cancellable(Some(&self.cancel))?;
        Ok(serde_json::from_str(&json)?)
    }

    /// Instruct the server to cancel the last received code. No response is read.
    pub fn cancel_code(&mut self) -> Result<()> {
        self.base.send(&Command::Cancel)
    }

    /// Instruct the server to pass the last received code on unmodified. No response is read.
    pub fn ignore_code(&mut self) -> Result<()> {
        self.base.send(&Command::Ignore)
    }

    /// Resolve the last received code with the given message. No response is read.
    pub fn resolve_code(&mut self, message_type: MessageType, content: Option<&str>) -> Result<()> {
        self.base.send(&Command::Resolve {
            message_type,
            content: content.map(String::from),
        })
    }
}

impl CommandApi for InterceptConnection {
    fn connection_mut(&mut self) -> &mut BaseConnection {
        &mut self.base
    }
}
