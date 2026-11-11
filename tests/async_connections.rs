#![cfg(feature = "tokio")]
mod common;
use common::*;
use dsf::asynchronous::*;
use dsf::*;
use serde_json::json;
use std::time::{Duration, Instant};

fn code_json(major: u32) -> String {
    format!(
        r#"{{"type":"M","channel":"SBC","majorNumber":{major},"flags":0,"parameters":[{{"letter":"S","value":"a}}b","isString":true,"isDriverId":false}}],"comment":null}}"#
    )
}

#[tokio::test]
async fn handshake_success() {
    let m = mock(|mut s| {
        let init = handshake(&mut s);
        assert_eq!(init, json!({"mode":"Command","version":12}));
    });
    let mut c = AsyncCommandConnection::new();
    c.connect(&m.path).await.unwrap();
    assert_eq!(c.base().id(), 42);
    m.handle.join().unwrap();
}

#[tokio::test]
async fn incompatible_version() {
    let m = mock(|mut s| send(&mut s, r#"{"version":11,"id":1}"#));
    let mut c = AsyncCommandConnection::new();
    match c.connect(&m.path).await {
        Err(Error::IncompatibleVersion { need: 12, got: 11 }) => {}
        other => panic!("unexpected {other:?}"),
    }
    assert!(!c.base().is_connected());
    m.handle.join().unwrap();
}

#[tokio::test]
async fn rejected_init() {
    let m = mock(|mut s| {
        send(&mut s, r#"{"version":12,"id":1}"#);
        read_json(&mut s);
        send(
            &mut s,
            r#"{"success":false,"errorType":"X","errorMessage":"nope"}"#,
        );
    });
    let mut c = AsyncCommandConnection::new();
    match c.connect(&m.path).await {
        Err(Error::ConnectionRejected {
            error_type,
            error_message,
        }) => {
            assert_eq!((error_type.as_str(), error_message.as_str()), ("X", "nope"));
        }
        other => panic!("unexpected {other:?}"),
    }
    m.handle.join().unwrap();
}

#[tokio::test]
async fn simple_code_round_trip_split_reply() {
    let m = mock(|mut s| {
        handshake(&mut s);
        assert_eq!(
            read_json(&mut s),
            json!({"command":"SimpleCode","code":"M115","channel":"SBC","executeAsynchronously":false})
        );
        send(&mut s, r#"{"success":true,"result":"FIRMWARE {x} "#);
        std::thread::sleep(Duration::from_millis(50));
        send(&mut s, r#"}"}"#);
    });
    let mut c = AsyncCommandConnection::new();
    c.connect(&m.path).await.unwrap();
    let r = c
        .perform_simple_code("M115", CodeChannel::Sbc, false)
        .await
        .unwrap();
    assert_eq!(r, "FIRMWARE {x} }");
    m.handle.join().unwrap();
}

#[tokio::test]
async fn error_mappings() {
    let m = mock(|mut s| {
        handshake(&mut s);
        read_json(&mut s);
        send(
            &mut s,
            r#"{"success":false,"errorType":"ArgumentException","errorMessage":"bad"}"#,
        );
        read_json(&mut s);
        send(
            &mut s,
            r#"{"success":false,"errorType":"TaskCanceledException","errorMessage":"cancelled"}"#,
        );
    });
    let mut c = AsyncCommandConnection::new();
    c.connect(&m.path).await.unwrap();
    match c.perform_simple_code("G1", CodeChannel::Sbc, false).await {
        Err(Error::InternalServer {
            command,
            error_type,
            error_message,
        }) => {
            assert_eq!(command, "SimpleCode");
            assert_eq!(error_type, "ArgumentException");
            assert_eq!(error_message, "bad");
        }
        other => panic!("unexpected {other:?}"),
    }
    match c.flush(CodeChannel::Sbc).await {
        Err(Error::TaskCanceled(msg)) => assert_eq!(msg, "cancelled"),
        other => panic!("unexpected {other:?}"),
    }
    m.handle.join().unwrap();
}

#[tokio::test]
async fn timeout_and_closed() {
    let m = mock(|mut s| {
        handshake(&mut s);
        read_json(&mut s);
        send(&mut s, r#"{"success":true}"#);
        read_json(&mut s); // never answered
        std::thread::sleep(Duration::from_millis(400));
    });
    let mut c = AsyncCommandConnection::new().with_timeout(Some(Duration::from_millis(150)));
    c.connect(&m.path).await.unwrap();
    c.lock_object_model().await.unwrap();
    let t = Instant::now();
    assert!(matches!(c.sync_object_model().await, Err(Error::Timeout)));
    assert!(t.elapsed() < Duration::from_millis(350));
    m.handle.join().unwrap();

    let m = mock(|mut s| {
        handshake(&mut s);
        read_json(&mut s);
    });
    let mut c = AsyncCommandConnection::new();
    c.connect(&m.path).await.unwrap();
    assert!(matches!(
        c.flush(CodeChannel::Sbc).await,
        Err(Error::ConnectionClosed)
    ));
    m.handle.join().unwrap();

    let mut c = AsyncCommandConnection::new();
    assert!(matches!(
        c.flush(CodeChannel::Sbc).await,
        Err(Error::NotConnected)
    ));
}

#[tokio::test]
async fn subscribe_acknowledges() {
    let m = mock(|mut s| {
        let init = handshake(&mut s);
        assert_eq!(
            init,
            json!({"mode":"Subscribe","version":12,"subscriptionMode":"Patch","filter":"","filters":null})
        );
        send(&mut s, r#"{"move":{"axes":[{"letter":"X"}]}}"#);
        assert_eq!(read_json(&mut s), json!({"command":"Acknowledge"}));
        send(&mut s, r#"{"move":{"axes":[{"homed":true}]}}"#);
        assert_eq!(read_json(&mut s), json!({"command":"Acknowledge"}));
    });
    let mut c = AsyncSubscribeConnection::new(SubscriptionMode::Patch, None);
    c.connect(&m.path).await.unwrap();
    let mut model = c.get_object_model().await.unwrap();
    let patch = c.get_object_model_patch().await.unwrap();
    object_model::apply_patch(&mut model, &serde_json::from_str(&patch).unwrap());
    assert_eq!(
        model,
        json!({"move":{"axes":[{"letter":"X","homed":true}]}})
    );
    m.handle.join().unwrap();
}

#[tokio::test]
async fn intercept_receive_and_resolve() {
    let m = mock(|mut s| {
        let init = handshake(&mut s);
        assert_eq!(init["mode"], "Intercept");
        assert_eq!(init["filters"], json!(["M1234"]));
        send(&mut s, &code_json(1234));
        assert_eq!(
            read_json(&mut s),
            json!({"command":"Resolve","type":1,"content":"hi"})
        );
        send(&mut s, &code_json(1));
        assert_eq!(read_json(&mut s), json!({"command":"Ignore"}));
        send(&mut s, &code_json(2));
        assert_eq!(read_json(&mut s), json!({"command":"Cancel"}));
    });
    let mut opts = InterceptOptions::new(InterceptionMode::Pre);
    opts.filters = Some(vec!["M1234".into()]);
    let mut c = AsyncInterceptConnection::new(opts);
    c.connect(&m.path).await.unwrap();
    let code = c.receive_code().await.unwrap();
    assert_eq!(code.major_number, Some(1234));
    assert_eq!(code.parameter('s').unwrap().string_value, "a}b");
    c.resolve_code(MessageType::Warning, Some("hi"))
        .await
        .unwrap();
    assert_eq!(c.receive_code().await.unwrap().major_number, Some(1));
    c.ignore_code().await.unwrap();
    assert_eq!(c.receive_code().await.unwrap().major_number, Some(2));
    c.cancel_code().await.unwrap();
    m.handle.join().unwrap();
}

#[tokio::test]
async fn receive_code_is_cancel_safe() {
    let full = code_json(9);
    let (first, second) = full.split_at(20);
    let (first, second) = (first.to_string(), second.to_string());
    let m = mock(move |mut s| {
        handshake(&mut s);
        send(&mut s, &first);
        std::thread::sleep(Duration::from_millis(400));
        send(&mut s, &second);
        std::thread::sleep(Duration::from_millis(200));
    });
    let mut c = AsyncInterceptConnection::new(InterceptOptions::new(InterceptionMode::Pre));
    c.connect(&m.path).await.unwrap();
    tokio::select! {
        r = c.receive_code() => panic!("unexpected {r:?}"),
        _ = tokio::time::sleep(Duration::from_millis(100)) => {}
    }
    // also via tokio::time::timeout
    assert!(
        tokio::time::timeout(Duration::from_millis(50), c.receive_code())
            .await
            .is_err()
    );
    assert_eq!(c.receive_code().await.unwrap().major_number, Some(9));
    m.handle.join().unwrap();
}

#[tokio::test]
async fn spawned_task_using_command_api_is_send() {
    let m = mock(|mut s| {
        handshake(&mut s);
        read_json(&mut s);
        send(&mut s, r#"{"success":true,"result":"ok"}"#);
        read_json(&mut s);
        send(&mut s, r#"{"success":true,"result":5}"#);
    });
    let mut c = AsyncCommandConnection::new();
    c.connect(&m.path).await.unwrap();
    let h = tokio::spawn(async move {
        let r = c
            .perform_simple_code("M115", CodeChannel::Sbc, false)
            .await?;
        let id = c
            .add_user_session(AccessLevel::ReadOnly, SessionType::Local, Some("x"))
            .await?;
        Ok::<_, Error>((r, id))
    });
    assert_eq!(h.await.unwrap().unwrap(), ("ok".to_string(), 5));
    m.handle.join().unwrap();
}
