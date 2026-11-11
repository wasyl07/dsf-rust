use crate::error::Result;
use crate::framing::take_frame;
use crate::http::{chmod_socket, prepare_socket_path, HttpResponse};
use crate::http::{HttpResponseType, ReceivedHttpRequest};
use crate::types::HttpEndpointType;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::task::JoinHandle;

/// A single connection of a custom async HTTP endpoint.
#[derive(Debug)]
pub struct AsyncHttpEndpointConnection {
    stream: UnixStream,
    buf: Vec<u8>,
    is_websocket: bool,
    debug: bool,
}

impl AsyncHttpEndpointConnection {
    /// Wrap an accepted stream.
    pub fn new(stream: UnixStream, is_websocket: bool, debug: bool) -> Self {
        AsyncHttpEndpointConnection {
            stream,
            buf: Vec::new(),
            is_websocket,
            debug,
        }
    }

    /// Close the connection (shuts down the write side; the stream is closed on drop).
    pub async fn close(&mut self) {
        let _ = self.stream.shutdown().await;
    }

    /// Read information about the HTTP request. This may fail. Cancel-safe.
    pub async fn read_request(&mut self) -> Result<ReceivedHttpRequest> {
        self.receive().await
    }

    /// Send a simple HTTP response and close the connection unless it is a WebSocket.
    pub async fn send_response(
        &mut self,
        status_code: u16,
        response: &str,
        response_type: HttpResponseType,
    ) -> Result<()> {
        let r = self
            .send(&HttpResponse {
                status_code,
                response,
                response_type,
            })
            .await;
        if !self.is_websocket {
            self.close().await;
        }
        r
    }

    /// Receive the raw text of one JSON object. Cancel-safe.
    pub async fn receive_json(&mut self) -> Result<String> {
        let json = loop {
            if let Some(frame) = take_frame(&mut self.buf)? {
                break frame;
            }
            if self.stream.read_buf(&mut self.buf).await? == 0 {
                return Err(crate::Error::ConnectionClosed);
            }
        };
        if self.debug {
            eprintln!("recv: {json}");
        }
        Ok(json)
    }

    /// Receive and deserialize one JSON object. Cancel-safe.
    pub async fn receive<T: DeserializeOwned>(&mut self) -> Result<T> {
        Ok(serde_json::from_str(&self.receive_json().await?)?)
    }

    /// Serialize and send an arbitrary object. Not cancel-safe.
    pub async fn send<T: Serialize + Sync + ?Sized>(&mut self, obj: &T) -> Result<()> {
        let json = serde_json::to_string(obj)?;
        if self.debug {
            eprintln!("send: {json}");
        }
        self.stream.write_all(json.as_bytes()).await?;
        Ok(())
    }
}

type BoxFuture = Pin<Box<dyn Future<Output = ()> + Send>>;
type Handler = Arc<dyn Fn(AsyncHttpEndpointConnection) -> BoxFuture + Send + Sync + 'static>;

/// A tokio UNIX socket server for a custom HTTP endpoint.
///
/// Each accepted connection is handled on its own task. [`close`](Self::close) and `Drop` abort
/// the accept task and remove the socket file; both work outside of a runtime context.
pub struct AsyncHttpEndpointUnixSocket {
    endpoint_type: HttpEndpointType,
    namespace: String,
    endpoint_path: String,
    socket_file: PathBuf,
    handler: Arc<Mutex<Option<Handler>>>,
    task: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for AsyncHttpEndpointUnixSocket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncHttpEndpointUnixSocket")
            .field("endpoint_type", &self.endpoint_type)
            .field("namespace", &self.namespace)
            .field("endpoint_path", &self.endpoint_path)
            .field("socket_file", &self.socket_file)
            .finish_non_exhaustive()
    }
}

impl AsyncHttpEndpointUnixSocket {
    /// Open a new UNIX socket on `socket_file` (a stale file is removed first) and start
    /// accepting. Must be called from within a tokio runtime.
    ///
    /// `backlog` is accepted for API compatibility; tokio listens with a fixed backlog.
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
        prepare_socket_path(&socket_file)?;
        let listener = UnixListener::bind(&socket_file)?;
        chmod_socket(&socket_file)?;

        let handler: Arc<Mutex<Option<Handler>>> = Arc::new(Mutex::new(None));
        let is_websocket = endpoint_type == HttpEndpointType::WebSocket;
        let task = {
            let handler = Arc::clone(&handler);
            tokio::spawn(async move {
                loop {
                    let stream = match listener.accept().await {
                        Ok((s, _)) => s,
                        Err(_) => {
                            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                            continue;
                        }
                    };
                    let handler = handler.lock().ok().and_then(|h| h.clone());
                    tokio::spawn(async move {
                        let mut conn =
                            AsyncHttpEndpointConnection::new(stream, is_websocket, debug);
                        match handler {
                            Some(h) => h(conn).await,
                            None => {
                                let _ = conn
                                    .send_response(
                                        500,
                                        "No event handler registered",
                                        HttpResponseType::StatusCode,
                                    )
                                    .await;
                            }
                        }
                    });
                }
            })
        };

        Ok(AsyncHttpEndpointUnixSocket {
            endpoint_type,
            namespace: namespace.to_string(),
            endpoint_path: path.to_string(),
            socket_file,
            handler,
            task: Some(task),
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

    /// Set the handler invoked (on its own task) for each accepted connection.
    ///
    /// The handler is responsible for reading the request and responding.
    pub fn set_endpoint_handler<F, Fut>(&self, handler: F)
    where
        F: Fn(AsyncHttpEndpointConnection) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let handler: Handler = Arc::new(move |c| Box::pin(handler(c)));
        if let Ok(mut h) = self.handler.lock() {
            *h = Some(handler);
        }
    }

    /// Stop accepting and remove the socket file. Idempotent; does not need a runtime.
    ///
    /// Connections already being handled keep running until their handler finishes.
    pub fn close(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
        let _ = std::fs::remove_file(&self.socket_file);
    }
}

impl Drop for AsyncHttpEndpointUnixSocket {
    fn drop(&mut self) {
        self.close();
    }
}
