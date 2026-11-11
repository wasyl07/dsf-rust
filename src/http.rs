//! Custom HTTP endpoints served over UNIX sockets.

use crate::error::Result;
use crate::framing::read_frame;
use crate::types::HttpEndpointType;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::net::Shutdown;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

/// Type of an HTTP response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HttpResponseType {
    /// Only a status code.
    #[serde(rename = "statuscode")]
    StatusCode,
    /// Plain text body.
    #[serde(rename = "plainText")]
    PlainText,
    /// JSON body.
    #[serde(rename = "json")]
    Json,
    /// File to serve.
    #[serde(rename = "file")]
    File,
    /// URI to redirect to.
    #[serde(rename = "uri")]
    Uri,
}

/// Notification sent by the web server when an HTTP request is received.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReceivedHttpRequest {
    /// Session id of the request.
    pub session_id: i64,
    /// Query parameters.
    pub queries: HashMap<String, String>,
    /// Request headers.
    pub headers: HashMap<String, String>,
    /// Content type.
    pub content_type: String,
    /// Request body.
    pub body: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HttpResponse<'a> {
    status_code: u16,
    response: &'a str,
    response_type: HttpResponseType,
}

/// A single connection of a custom HTTP endpoint.
#[derive(Debug)]
pub struct HttpEndpointConnection {
    stream: UnixStream,
    buf: Vec<u8>,
    is_websocket: bool,
    debug: bool,
}

impl HttpEndpointConnection {
    /// Wrap an accepted stream.
    pub fn new(stream: UnixStream, is_websocket: bool, debug: bool) -> Self {
        HttpEndpointConnection {
            stream,
            buf: Vec::new(),
            is_websocket,
            debug,
        }
    }

    /// Close the connection.
    pub fn close(&mut self) {
        let _ = self.stream.shutdown(Shutdown::Both);
    }

    /// Read information about the HTTP request. This may fail.
    pub fn read_request(&mut self) -> Result<ReceivedHttpRequest> {
        self.receive()
    }

    /// Send a simple HTTP response and close the connection unless it is a WebSocket.
    pub fn send_response(
        &mut self,
        status_code: u16,
        response: &str,
        response_type: HttpResponseType,
    ) -> Result<()> {
        let r = self.send(&HttpResponse {
            status_code,
            response,
            response_type,
        });
        if !self.is_websocket {
            self.close();
        }
        r
    }

    /// Receive the raw text of one JSON object.
    pub fn receive_json(&mut self) -> Result<String> {
        let json = read_frame(&self.stream, &mut self.buf, None, None)?;
        if self.debug {
            eprintln!("recv: {json}");
        }
        Ok(json)
    }

    /// Receive and deserialize one JSON object.
    pub fn receive<T: DeserializeOwned>(&mut self) -> Result<T> {
        Ok(serde_json::from_str(&self.receive_json()?)?)
    }

    /// Serialize and send an arbitrary object.
    pub fn send<T: Serialize + ?Sized>(&mut self, obj: &T) -> Result<()> {
        let json = serde_json::to_string(obj)?;
        if self.debug {
            eprintln!("send: {json}");
        }
        self.stream.write_all(json.as_bytes())?;
        Ok(())
    }
}

type Handler = Arc<dyn Fn(HttpEndpointConnection) + Send + Sync + 'static>;

/// A UNIX socket server for a custom HTTP endpoint.
///
/// Each accepted connection is handled on its own thread. Dropping the value (or calling
/// [`close`](Self::close)) stops the listener thread and removes the socket file.
pub struct HttpEndpointUnixSocket {
    endpoint_type: HttpEndpointType,
    namespace: String,
    endpoint_path: String,
    socket_file: PathBuf,
    handler: Arc<Mutex<Option<Handler>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for HttpEndpointUnixSocket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpEndpointUnixSocket")
            .field("endpoint_type", &self.endpoint_type)
            .field("namespace", &self.namespace)
            .field("endpoint_path", &self.endpoint_path)
            .field("socket_file", &self.socket_file)
            .finish_non_exhaustive()
    }
}

impl HttpEndpointUnixSocket {
    /// Open a new UNIX socket on `socket_file` (a stale file is removed first) and start accepting.
    ///
    /// `backlog` is accepted for API compatibility; the standard library listens with a fixed backlog.
    pub fn new(
        endpoint_type: HttpEndpointType,
        namespace: &str,
        path: &str,
        socket_file: impl AsRef<Path>,
        backlog: u32,
        debug: bool,
    ) -> Result<Self> {
        let _ = backlog;
        let socket_file = socket_file.as_ref().to_path_buf();
        match std::fs::remove_file(&socket_file) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        let listener = UnixListener::bind(&socket_file)?;
        let mut perms = std::fs::metadata(&socket_file)?.permissions();
        perms.set_mode(perms.mode() | 0o060);
        std::fs::set_permissions(&socket_file, perms)?;

        let handler: Arc<Mutex<Option<Handler>>> = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let is_websocket = endpoint_type == HttpEndpointType::WebSocket;

        let thread = {
            let handler = Arc::clone(&handler);
            let stop = Arc::clone(&stop);
            thread::spawn(move || {
                for stream in listener.incoming() {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    let Ok(stream) = stream else { continue };
                    let handler = handler.lock().ok().and_then(|h| h.clone());
                    thread::spawn(move || {
                        let mut conn = HttpEndpointConnection::new(stream, is_websocket, debug);
                        match handler {
                            Some(h) => h(conn),
                            None => {
                                let _ = conn.send_response(
                                    500,
                                    "No event handler registered",
                                    HttpResponseType::StatusCode,
                                );
                            }
                        }
                    });
                }
            })
        };

        Ok(HttpEndpointUnixSocket {
            endpoint_type,
            namespace: namespace.to_string(),
            endpoint_path: path.to_string(),
            socket_file,
            handler,
            stop,
            thread: Some(thread),
        })
    }

    /// Endpoint type.
    pub fn endpoint_type(&self) -> HttpEndpointType {
        self.endpoint_type
    }

    /// Endpoint namespace.
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// Endpoint path.
    pub fn endpoint_path(&self) -> &str {
        &self.endpoint_path
    }

    /// Path of the socket file.
    pub fn socket_file(&self) -> &Path {
        &self.socket_file
    }

    /// Set the handler invoked (on its own thread) for each accepted connection.
    ///
    /// The handler is responsible for reading the request and responding.
    pub fn set_endpoint_handler<F>(&self, handler: F)
    where
        F: Fn(HttpEndpointConnection) + Send + Sync + 'static,
    {
        if let Ok(mut h) = self.handler.lock() {
            *h = Some(Arc::new(handler));
        }
    }

    /// Stop the listener thread and remove the socket file. Idempotent.
    pub fn close(&mut self) {
        if let Some(thread) = self.thread.take() {
            self.stop.store(true, Ordering::SeqCst);
            // Wake the blocking accept().
            let _ = UnixStream::connect(&self.socket_file);
            let _ = thread.join();
        }
        match std::fs::remove_file(&self.socket_file) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => {}
        }
    }
}

impl Drop for HttpEndpointUnixSocket {
    fn drop(&mut self) {
        self.close();
    }
}
