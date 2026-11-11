mod common;
use dsf::*;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::time::Duration;

fn read_all(s: &mut UnixStream) -> String {
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    out
}

#[test]
fn handler_receives_request_and_responds() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ep.sock");
    let mut ep =
        HttpEndpointUnixSocket::new(HttpEndpointType::Get, "custom", "getIt", &path, 4, false)
            .unwrap();
    let (tx, rx) = mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    ep.set_endpoint_handler(move |mut conn| {
        let req = conn.read_request().unwrap();
        tx.lock().unwrap().send(req).unwrap();
        conn.send_response(200, "so happy", HttpResponseType::PlainText)
            .unwrap();
    });

    let mut c = UnixStream::connect(&path).unwrap();
    c.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let body = "{\"x\":\"}{\"}"; // braces in body
    let req = json!({"sessionId":3,"queries":{"a":"b"},"headers":{"H":"v"},"contentType":"text/plain","body":body});
    let text = req.to_string();
    let (a, b) = text.split_at(10);
    c.write_all(a.as_bytes()).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    c.write_all(b.as_bytes()).unwrap();

    let reply = read_all(&mut c); // handler closes the connection
    let v: Value = serde_json::from_str(&reply).unwrap();
    assert_eq!(
        v,
        json!({"statusCode":200,"response":"so happy","responseType":"plainText"})
    );

    let got = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(got.session_id, 3);
    assert_eq!(got.queries["a"], "b");
    assert_eq!(got.headers["H"], "v");
    assert_eq!(got.content_type, "text/plain");
    assert_eq!(got.body, body);

    ep.close();
    assert!(!path.exists());
}

#[test]
fn no_handler_gives_500() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ep.sock");
    let _ep =
        HttpEndpointUnixSocket::new(HttpEndpointType::Post, "n", "p", &path, 4, false).unwrap();
    let mut c = UnixStream::connect(&path).unwrap();
    c.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let v: Value = serde_json::from_str(&read_all(&mut c)).unwrap();
    assert_eq!(
        v,
        json!({"statusCode":500,"response":"No event handler registered","responseType":"statuscode"})
    );
}

#[test]
fn close_removes_socket_and_stops_thread() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ep.sock");
    // stale file is replaced
    std::fs::write(&path, b"stale").unwrap();
    let mut ep =
        HttpEndpointUnixSocket::new(HttpEndpointType::Get, "n", "p", &path, 4, false).unwrap();
    assert!(path.exists());
    ep.close(); // joins the accept thread; would hang if it did not terminate
    assert!(!path.exists());
    ep.close(); // idempotent
    assert!(UnixStream::connect(&path).is_err());
}

#[test]
fn drop_removes_socket() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ep.sock");
    drop(HttpEndpointUnixSocket::new(HttpEndpointType::Get, "n", "p", &path, 4, false).unwrap());
    assert!(!path.exists());
}

#[test]
fn add_http_endpoint_via_command_connection() {
    let dir = tempfile::tempdir().unwrap();
    let ep_path = dir.path().join("endpoint.sock");
    let ep_str = ep_path.to_str().unwrap().to_string();
    let m = common::mock(move |mut s| {
        common::handshake(&mut s);
        assert_eq!(
            common::read_json(&mut s),
            json!({"command":"AddHttpEndpoint","endpointType":"GET","namespace":"custom","path":"getIt","isUploadRequest":false})
        );
        common::send(&mut s, &json!({"success":true,"result":ep_str}).to_string());
    });
    let mut c = CommandConnection::new();
    c.connect(&m.path).unwrap();
    let mut ep = c
        .add_http_endpoint_default(HttpEndpointType::Get, "custom", "getIt")
        .unwrap();
    assert!(ep_path.exists());
    ep.close();
    assert!(!ep_path.exists());
    m.handle.join().unwrap();
}
