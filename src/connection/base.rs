use crate::commands::{Command, InitMessage};
use crate::error::{Error, Result};
use crate::framing::read_frame;
use crate::PROTOCOL_VERSION;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

#[derive(Deserialize)]
struct ServerInit {
    version: u32,
    #[serde(default)]
    id: i64,
}

/// A response to a command, as sent by the server.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    /// Whether the command succeeded.
    pub success: bool,
    /// Result of the command (`null` if absent).
    #[serde(default)]
    pub result: Value,
    /// Error type (on failure).
    #[serde(default)]
    pub error_type: Option<String>,
    /// Error message (on failure).
    #[serde(default)]
    pub error_message: Option<String>,
}

/// Low-level connection shared by all connection types.
#[derive(Debug)]
pub struct BaseConnection {
    stream: Option<UnixStream>,
    id: i64,
    buf: Vec<u8>,
    timeout: Option<Duration>,
    debug: bool,
}

impl BaseConnection {
    /// Create an unconnected base connection.
    pub fn new(timeout: Option<Duration>, debug: bool) -> Self {
        BaseConnection {
            stream: None,
            id: 0,
            buf: Vec::new(),
            timeout,
            debug,
        }
    }

    /// Connection id assigned by the server.
    pub fn id(&self) -> i64 {
        self.id
    }

    /// Whether a socket is currently open.
    pub fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    /// Current receive timeout.
    pub fn timeout(&self) -> Option<Duration> {
        self.timeout
    }

    /// Set the receive timeout (`None` waits indefinitely). The timeout is overall per receive.
    pub fn set_timeout(&mut self, timeout: Option<Duration>) {
        self.timeout = timeout;
    }

    /// Whether debug output is enabled.
    pub fn debug(&self) -> bool {
        self.debug
    }

    /// Enable or disable printing of sent/received JSON to stderr.
    pub fn set_debug(&mut self, debug: bool) {
        self.debug = debug;
    }

    /// Connect to the socket, perform the version handshake and select the connection mode.
    pub fn connect(&mut self, init: &InitMessage, socket_file: impl AsRef<Path>) -> Result<()> {
        self.close();
        let stream = UnixStream::connect(socket_file)?;
        self.stream = Some(stream);
        self.buf.clear();
        let r = self.handshake(init);
        if r.is_err() {
            self.close();
        }
        r
    }

    fn handshake(&mut self, init: &InitMessage) -> Result<()> {
        let hello: ServerInit = self.receive()?;
        if hello.version < PROTOCOL_VERSION {
            return Err(Error::IncompatibleVersion {
                need: PROTOCOL_VERSION,
                got: hello.version,
            });
        }
        self.id = hello.id;
        self.send(init)?;
        let response = self.receive_response()?;
        if !response.success {
            return Err(Error::ConnectionRejected {
                error_type: response.error_type.unwrap_or_default(),
                error_message: response.error_message.unwrap_or_default(),
            });
        }
        Ok(())
    }

    /// Close the connection.
    pub fn close(&mut self) {
        self.stream = None;
    }

    /// Serialize `msg` as compact JSON and send it.
    pub fn send<T: Serialize + ?Sized>(&mut self, msg: &T) -> Result<()> {
        let json = serde_json::to_string(msg)?;
        let debug = self.debug;
        let stream = self.stream.as_mut().ok_or(Error::NotConnected)?;
        if debug {
            eprintln!("send: {json}");
        }
        stream.write_all(json.as_bytes())?;
        Ok(())
    }

    /// Receive the raw text of exactly one JSON object.
    pub fn receive_json(&mut self) -> Result<String> {
        self.receive_json_cancellable(None)
    }

    /// Like [`receive_json`](Self::receive_json), but returns [`Error::Cancelled`] (resetting
    /// the flag) once `cancel` is set. A complete frame already buffered is returned first.
    pub fn receive_json_cancellable(&mut self, cancel: Option<&AtomicBool>) -> Result<String> {
        let stream = self.stream.as_ref().ok_or(Error::NotConnected)?;
        let json = read_frame(stream, &mut self.buf, self.timeout, cancel)?;
        if self.debug {
            eprintln!("recv: {json}");
        }
        Ok(json)
    }

    /// Receive and deserialize one JSON object.
    pub fn receive<T: DeserializeOwned>(&mut self) -> Result<T> {
        let json = self.receive_json()?;
        Ok(serde_json::from_str(&json)?)
    }

    /// Receive a command response.
    pub fn receive_response(&mut self) -> Result<Response> {
        self.receive()
    }

    /// Send a command and return its `result` (`null` if the response had none).
    pub fn perform_command(&mut self, command: &Command) -> Result<Value> {
        self.send(command)?;
        let response = self.receive_response()?;
        if response.success {
            return Ok(response.result);
        }
        let error_type = response.error_type.unwrap_or_default();
        let error_message = response.error_message.unwrap_or_default();
        if error_type == "TaskCanceledException" {
            return Err(Error::TaskCanceled(error_message));
        }
        let name = serde_json::to_value(command)
            .ok()
            .and_then(|v| v.get("command").and_then(|c| c.as_str().map(String::from)))
            .unwrap_or_default();
        Err(Error::InternalServer {
            command: name,
            error_type,
            error_message,
        })
    }
}
