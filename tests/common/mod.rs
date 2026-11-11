#![allow(dead_code)]

use serde_json::Value;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::thread::JoinHandle;
use tempfile::TempDir;

pub struct Mock {
    pub dir: TempDir,
    pub path: PathBuf,
    pub handle: JoinHandle<()>,
}

/// Start a mock server handling exactly one connection with `f`.
pub fn mock<F>(f: F) -> Mock
where
    F: FnOnce(UnixStream) + Send + 'static,
{
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dcs.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let handle = std::thread::spawn(move || {
        let (s, _) = listener.accept().unwrap();
        f(s);
    });
    Mock { dir, path, handle }
}

/// Read one JSON value sent by the client.
pub fn read_json(s: &mut UnixStream) -> Value {
    let mut data = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        if let Ok(v) = serde_json::from_slice::<Value>(&data) {
            return v;
        }
        let n = s.read(&mut chunk).unwrap();
        assert!(
            n > 0,
            "client closed; got {:?}",
            String::from_utf8_lossy(&data)
        );
        data.extend_from_slice(&chunk[..n]);
    }
}

pub fn send(s: &mut UnixStream, text: &str) {
    s.write_all(text.as_bytes()).unwrap();
}

/// Server hello + read init + reply success; returns the init message.
pub fn handshake(s: &mut UnixStream) -> Value {
    send(s, r#"{"version":12,"id":42}"#);
    let init = read_json(s);
    send(s, r#"{"success":true}"#);
    init
}

/// Start a mock server accepting exactly `n` connections; `f(index, stream)` runs on a thread each.
pub fn mock_n<F>(n: usize, f: F) -> Mock
where
    F: Fn(usize, UnixStream) + Send + Sync + 'static,
{
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dcs.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let f = std::sync::Arc::new(f);
    let handle = std::thread::spawn(move || {
        let mut handles = Vec::new();
        for i in 0..n {
            let (s, _) = listener.accept().unwrap();
            let f = f.clone();
            handles.push(std::thread::spawn(move || f(i, s)));
        }
        for h in handles {
            h.join().unwrap();
        }
    });
    Mock { dir, path, handle }
}

/// Block until the client closes the stream.
pub fn drain(s: &mut UnixStream) {
    let mut b = [0u8; 256];
    while s.read(&mut b).map(|n| n > 0).unwrap_or(false) {}
}
