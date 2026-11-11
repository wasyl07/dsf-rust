use crate::commands::{Command, InitMessage};
use crate::connection::{Response, ServerInit};
use crate::error::{Error, Result};
use crate::framing::take_frame;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use std::future::Future;
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

/// Read one JSON value, appending to `buf` directly so that dropping the future loses nothing.
async fn read_frame(stream: &mut UnixStream, buf: &mut Vec<u8>) -> Result<String> {
    loop {
        if let Some(frame) = take_frame(buf)? {
            return Ok(frame);
        }
        // `read_buf` appends to `buf` in the same poll that reads the bytes: cancel-safe.
        if stream.read_buf(buf).await? == 0 {
            return if buf.iter().all(|b| b.is_ascii_whitespace()) {
                Err(Error::ConnectionClosed)
            } else {
                Err(Error::Io(std::io::ErrorKind::UnexpectedEof.into()))
            };
        }
    }
}

pub(crate) async fn with_timeout<T>(
    timeout: Option<Duration>,
    fut: impl Future<Output = Result<T>>,
) -> Result<T> {
    match timeout {
        Some(t) => tokio::time::timeout(t, fut)
            .await
            .map_err(|_| Error::Timeout)?,
        None => fut.await,
    }
}

/// Low-level async connection shared by all async connection types.
///
/// `receive`, `receive_json` and `receive_response` are cancel-safe: if the future is dropped
/// (e.g. by `tokio::select!` or `tokio::time::timeout`), no received bytes are lost and the next
/// call returns the complete frame. `send` is **not** cancel-safe: cancelling it may leave a
/// partial message on the wire, so it must be driven to completion.
#[derive(Debug)]
pub struct AsyncBaseConnection {
    stream: Option<UnixStream>,
    id: i64,
    buf: Vec<u8>,
    timeout: Option<Duration>,
    debug: bool,
}

impl AsyncBaseConnection {
    /// Create an unconnected base connection.
    pub fn new(timeout: Option<Duration>, debug: bool) -> Self {
        AsyncBaseConnection {
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
    pub async fn connect(
        &mut self,
        init: &InitMessage,
        socket_file: impl AsRef<Path> + Send,
    ) -> Result<()> {
        self.close();
        let path = socket_file.as_ref().to_path_buf();
        self.stream = Some(UnixStream::connect(path).await?);
        self.buf.clear();
        let r = self.handshake(init).await;
        if r.is_err() {
            self.close();
        }
        r
    }

    async fn handshake(&mut self, init: &InitMessage) -> Result<()> {
        let hello: ServerInit = self.receive().await?;
        hello.check()?;
        self.id = hello.id;
        self.send(init).await?;
        self.receive_response().await?.into_init_result()
    }

    /// Close the connection.
    pub fn close(&mut self) {
        self.stream = None;
    }

    /// Serialize `msg` as compact JSON and send it. Not cancel-safe.
    pub async fn send<T: Serialize + Sync + ?Sized>(&mut self, msg: &T) -> Result<()> {
        let json = serde_json::to_string(msg)?;
        if self.debug {
            eprintln!("send: {json}");
        }
        let stream = self.stream.as_mut().ok_or(Error::NotConnected)?;
        stream.write_all(json.as_bytes()).await?;
        Ok(())
    }

    /// Receive the raw text of exactly one JSON object. Cancel-safe.
    pub async fn receive_json(&mut self) -> Result<String> {
        let timeout = self.timeout;
        let stream = self.stream.as_mut().ok_or(Error::NotConnected)?;
        let json = with_timeout(timeout, read_frame(stream, &mut self.buf)).await?;
        if self.debug {
            eprintln!("recv: {json}");
        }
        Ok(json)
    }

    /// Receive and deserialize one JSON object. Cancel-safe.
    pub async fn receive<T: DeserializeOwned>(&mut self) -> Result<T> {
        let json = self.receive_json().await?;
        Ok(serde_json::from_str(&json)?)
    }

    /// Receive a command response. Cancel-safe.
    pub async fn receive_response(&mut self) -> Result<Response> {
        self.receive().await
    }

    /// Send a command and return its `result` (`null` if the response had none).
    ///
    /// Do not cancel this future: the request may already be sent and the response would then
    /// be left unread.
    pub async fn perform_command(&mut self, command: &Command) -> Result<Value> {
        self.send(command).await?;
        self.receive_response().await?.into_command_result(command)
    }
}
