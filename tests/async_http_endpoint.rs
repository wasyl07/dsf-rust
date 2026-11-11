#![cfg(feature = "tokio")]
mod common;
use dsf::asynchronous::*;
use dsf::*;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

async fn read_all(s: &mut UnixStream) -> String {
    let mut out = String::new();
    tokio::time::timeout(Duration::from_secs(5), s.read_to_string(&mut out))
        .await
        .unwrap()
        .unwrap();
    out
}

#[tokio::test]
async fn handler_receives_request_and_responds() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ep.sock");
    let mut ep =
        AsyncHttpEndpointUnixSocket::new(HttpEndpointType::Get, "custom", "getIt", &path, 4, false)
            .unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    ep.set_endpoint_handler(move |mut conn| {
        let tx = tx.clone();
        async move {
            let req = conn.read_request().await.unwrap();
            tx.send(req).unwrap();
            conn.send_response(200, "so happy", HttpResponseType::PlainText)
                .await
                .unwrap();
        }
    });

    let mut c = UnixStream::connect(&path).await.unwrap();
    let body = "{\"x\":\"}{\"}";
    let text = json!({"sessionId":3,"queries":{"a":"b"},"headers":{"H":"v"},"contentType":"text/plain","body":body})
        .to_string();
    let (a, b) = text.split_at(10);
    c.write_all(a.as_bytes()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    c.write_all(b.as_bytes()).await.unwrap();

    let v: Value = serde_json::from_str(&read_all(&mut c).await).unwrap();
    assert_eq!(
        v,
        json!({"statusCode":200,"response":"so happy","responseType":"plainText"})
    );
    let got = rx.recv().await.unwrap();
    assert_eq!(got.session_id, 3);
    assert_eq!(got.queries["a"], "b");
    assert_eq!(got.headers["H"], "v");
    assert_eq!(got.body, body);

    ep.close();
    assert!(!path.exists());
    ep.close();
}

#[tokio::test]
async fn no_handler_gives_500() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ep.sock");
    let _ep = AsyncHttpEndpointUnixSocket::new(HttpEndpointType::Post, "n", "p", &path, 4, false)
        .unwrap();
    let mut c = UnixStream::connect(&path).await.unwrap();
    let v: Value = serde_json::from_str(&read_all(&mut c).await).unwrap();
    assert_eq!(
        v,
        json!({"statusCode":500,"response":"No event handler registered","responseType":"statuscode"})
    );
}

#[tokio::test]
async fn close_and_drop_remove_socket_and_replace_stale() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ep.sock");
    std::fs::write(&path, b"stale").unwrap();
    let mut ep =
        AsyncHttpEndpointUnixSocket::new(HttpEndpointType::Get, "n", "p", &path, 4, false).unwrap();
    assert!(path.exists());
    ep.close();
    assert!(!path.exists());
    assert!(UnixStream::connect(&path).await.is_err());

    let ep =
        AsyncHttpEndpointUnixSocket::new(HttpEndpointType::Get, "n", "p", &path, 4, false).unwrap();
    drop(ep);
    assert!(!path.exists());
}

#[test]
fn drop_outside_runtime() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ep.sock");
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let ep = rt.block_on(async {
        AsyncHttpEndpointUnixSocket::new(HttpEndpointType::Get, "n", "p", &path, 4, false).unwrap()
    });
    drop(ep); // no runtime context here
    assert!(!path.exists());
}

#[tokio::test]
async fn add_http_endpoint_via_command_connection() {
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
    let mut c = AsyncCommandConnection::new();
    c.connect(&m.path).await.unwrap();
    let mut ep = c
        .add_http_endpoint_default(HttpEndpointType::Get, "custom", "getIt")
        .await
        .unwrap();
    assert!(ep_path.exists());
    ep.close();
    assert!(!ep_path.exists());
    m.handle.join().unwrap();
}
