mod common;
use common::*;
use dsf::*;
use serde_json::json;
use std::time::{Duration, Instant};

#[test]
fn command_handshake_success() {
    let m = mock(|mut s| {
        let init = handshake(&mut s);
        assert_eq!(init, json!({"mode":"Command","version":12}));
    });
    let mut c = CommandConnection::new();
    c.connect(&m.path).unwrap();
    assert_eq!(c.base().id(), 42);
    m.handle.join().unwrap();
}

#[test]
fn incompatible_version() {
    let m = mock(|mut s| send(&mut s, r#"{"version":11,"id":1}"#));
    let mut c = CommandConnection::new();
    match c.connect(&m.path) {
        Err(Error::IncompatibleVersion { need: 12, got: 11 }) => {}
        other => panic!("unexpected {other:?}"),
    }
    assert!(!c.base().is_connected());
    m.handle.join().unwrap();
}

#[test]
fn rejected_init() {
    let m = mock(|mut s| {
        send(&mut s, r#"{"version":12,"id":1}"#);
        read_json(&mut s);
        send(
            &mut s,
            r#"{"success":false,"errorType":"IncompatibleVersion","errorMessage":"nope"}"#,
        );
    });
    let mut c = CommandConnection::new();
    match c.connect(&m.path) {
        Err(Error::ConnectionRejected {
            error_type,
            error_message,
        }) => {
            assert_eq!(error_type, "IncompatibleVersion");
            assert_eq!(error_message, "nope");
        }
        other => panic!("unexpected {other:?}"),
    }
    m.handle.join().unwrap();
}

#[test]
fn simple_code_round_trip() {
    let m = mock(|mut s| {
        handshake(&mut s);
        let cmd = read_json(&mut s);
        assert_eq!(
            cmd,
            json!({"command":"SimpleCode","code":"M115","channel":"SBC","executeAsynchronously":false})
        );
        // reply with braces inside a string, split across two writes
        send(&mut s, r#"{"success":true,"result":"FIRMWARE {x} "#);
        std::thread::sleep(Duration::from_millis(50));
        send(&mut s, r#"}"}"#);
    });
    let mut c = CommandConnection::new();
    c.connect(&m.path).unwrap();
    let r = c
        .perform_simple_code("M115", CodeChannel::Sbc, false)
        .unwrap();
    assert_eq!(r, "FIRMWARE {x} }");
    m.handle.join().unwrap();
}

#[test]
fn error_response_maps_to_internal_server() {
    let m = mock(|mut s| {
        handshake(&mut s);
        read_json(&mut s);
        send(
            &mut s,
            r#"{"success":false,"errorType":"ArgumentException","errorMessage":"bad"}"#,
        );
    });
    let mut c = CommandConnection::new();
    c.connect(&m.path).unwrap();
    match c.perform_simple_code("G1", CodeChannel::Sbc, false) {
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
    m.handle.join().unwrap();
}

#[test]
fn task_canceled_mapping() {
    let m = mock(|mut s| {
        handshake(&mut s);
        read_json(&mut s);
        send(
            &mut s,
            r#"{"success":false,"errorType":"TaskCanceledException","errorMessage":"cancelled"}"#,
        );
    });
    let mut c = CommandConnection::new();
    c.connect(&m.path).unwrap();
    match c.flush(CodeChannel::Sbc) {
        Err(Error::TaskCanceled(msg)) => assert_eq!(msg, "cancelled"),
        other => panic!("unexpected {other:?}"),
    }
    m.handle.join().unwrap();
}

#[test]
fn missing_result_is_tolerated_and_timeout_works() {
    let m = mock(|mut s| {
        handshake(&mut s);
        read_json(&mut s);
        send(&mut s, r#"{"success":true}"#);
        read_json(&mut s); // never answered
        std::thread::sleep(Duration::from_millis(400));
    });
    let mut c = CommandConnection::new().with_timeout(Some(Duration::from_millis(150)));
    c.connect(&m.path).unwrap();
    c.lock_object_model().unwrap();
    let t = Instant::now();
    assert!(matches!(c.sync_object_model(), Err(Error::Timeout)));
    assert!(t.elapsed() < Duration::from_millis(350));
    m.handle.join().unwrap();
}

#[test]
fn connection_closed() {
    let m = mock(|mut s| {
        handshake(&mut s);
        read_json(&mut s);
    });
    let mut c = CommandConnection::new();
    c.connect(&m.path).unwrap();
    assert!(matches!(
        c.flush(CodeChannel::Sbc),
        Err(Error::ConnectionClosed)
    ));
    m.handle.join().unwrap();
}

#[test]
fn not_connected() {
    let mut c = CommandConnection::new();
    assert!(matches!(
        c.flush(CodeChannel::Sbc),
        Err(Error::NotConnected)
    ));
}

#[test]
fn serialized_object_model() {
    let m = mock(|mut s| {
        handshake(&mut s);
        assert_eq!(read_json(&mut s), json!({"command":"GetObjectModel"}));
        send(&mut s, r#"{"success":true,"result":{"a":1}}"#);
    });
    let mut c = CommandConnection::new();
    c.connect(&m.path).unwrap();
    let raw = c.get_serialized_object_model().unwrap();
    assert_eq!(raw, r#"{"success":true,"result":{"a":1}}"#);
    m.handle.join().unwrap();
}

#[test]
fn subscribe_acknowledges() {
    let m = mock(|mut s| {
        let init = handshake(&mut s);
        assert_eq!(
            init,
            json!({"mode":"Subscribe","version":12,"subscriptionMode":"Patch","filter":"","filters":["move/**"]})
        );
        send(&mut s, r#"{"move":{"axes":[{"letter":"X"}]}}"#);
        assert_eq!(read_json(&mut s), json!({"command":"Acknowledge"}));
        send(&mut s, r#"{"move":{"axes":[{"homed":true}]}}"#);
        assert_eq!(read_json(&mut s), json!({"command":"Acknowledge"}));
    });
    let mut c = SubscribeConnection::new(SubscriptionMode::Patch, Some(vec!["move/**".into()]));
    c.connect(&m.path).unwrap();
    let mut model = c.get_object_model().unwrap();
    let patch = c.get_object_model_patch().unwrap();
    object_model::apply_patch(&mut model, &serde_json::from_str(&patch).unwrap());
    assert_eq!(
        model,
        json!({"move":{"axes":[{"letter":"X","homed":true}]}})
    );
    m.handle.join().unwrap();
}

fn code_json(major: u32) -> String {
    format!(
        r#"{{"type":"M","channel":"SBC","majorNumber":{major},"flags":0,"parameters":[{{"letter":"S","value":"a}}b","isString":true,"isDriverId":false}}],"comment":null}}"#
    )
}

#[test]
fn intercept_receive_and_resolve() {
    let m = mock(|mut s| {
        let init = handshake(&mut s);
        assert_eq!(init["mode"], "Intercept");
        assert_eq!(init["interceptionMode"], "Pre");
        assert_eq!(init["filters"], json!(["M1234"]));
        assert_eq!(init["autoFlush"], true);
        assert_eq!(init["autoEvaluateExpressions"], true);
        assert_eq!(init["priorityCodes"], false);
        assert_eq!(init["channels"].as_array().unwrap().len(), 15);
        send(&mut s, &code_json(1234));
        assert_eq!(
            read_json(&mut s),
            json!({"command":"Resolve","type":1,"content":"hi"})
        );
        send(&mut s, &code_json(1));
        assert_eq!(read_json(&mut s), json!({"command":"Ignore"}));
        send(&mut s, &code_json(2));
        assert_eq!(read_json(&mut s), json!({"command":"Cancel"}));
        send(&mut s, &code_json(3));
        assert_eq!(
            read_json(&mut s),
            json!({"command":"Resolve","type":0,"content":null})
        );
    });
    let mut opts = InterceptOptions::new(InterceptionMode::Pre);
    opts.filters = Some(vec!["M1234".into()]);
    let mut c = InterceptConnection::new(opts);
    c.connect(&m.path).unwrap();
    let code = c.receive_code().unwrap();
    assert_eq!(code.code_type, CodeType::MCode);
    assert_eq!(code.major_number, Some(1234));
    assert_eq!(code.parameter('s').unwrap().string_value, "a}b");
    c.resolve_code(MessageType::Warning, Some("hi")).unwrap();
    assert_eq!(c.receive_code().unwrap().major_number, Some(1));
    c.ignore_code().unwrap();
    assert_eq!(c.receive_code().unwrap().major_number, Some(2));
    c.cancel_code().unwrap();
    assert_eq!(c.receive_code().unwrap().major_number, Some(3));
    c.resolve_code(MessageType::Success, None).unwrap();
    m.handle.join().unwrap();
}

fn cancel_scenario<F>(server: F) -> (InterceptConnection, Mock)
where
    F: FnOnce(std::os::unix::net::UnixStream) + Send + 'static,
{
    let m = mock(move |mut s| {
        handshake(&mut s);
        server(s);
    });
    let mut c = InterceptConnection::new(InterceptOptions::new(InterceptionMode::Pre));
    c.connect(&m.path).unwrap();
    (c, m)
}

#[test]
fn cancel_blocked_receive_then_continue() {
    let (mut c, m) = cancel_scenario(|mut s| {
        std::thread::sleep(Duration::from_millis(600));
        send(&mut s, &code_json(7));
        std::thread::sleep(Duration::from_millis(200));
    });
    let canceller = c.canceller();
    let t = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        canceller.cancel();
    });
    let start = Instant::now();
    assert!(matches!(c.receive_code(), Err(Error::Cancelled)));
    assert!(start.elapsed() < Duration::from_secs(1));
    t.join().unwrap();
    // connection still usable, flag was reset
    let code = c.receive_code().unwrap();
    assert_eq!(code.major_number, Some(7));
    m.handle.join().unwrap();
}

#[test]
fn cancel_before_receive_returns_immediately() {
    let (mut c, m) = cancel_scenario(|_s| std::thread::sleep(Duration::from_millis(300)));
    c.canceller().cancel();
    let start = Instant::now();
    assert!(matches!(c.receive_code(), Err(Error::Cancelled)));
    assert!(start.elapsed() < Duration::from_millis(200));
    m.handle.join().unwrap();
}

#[test]
fn cancel_keeps_partial_data() {
    let full = code_json(9);
    let (first, second) = full.split_at(20);
    let (first, second) = (first.to_string(), second.to_string());
    let (mut c, m) = cancel_scenario(move |mut s| {
        send(&mut s, &first);
        std::thread::sleep(Duration::from_millis(500));
        send(&mut s, &second);
        std::thread::sleep(Duration::from_millis(100));
    });
    let canceller = c.canceller();
    let t = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        canceller.cancel();
    });
    assert!(matches!(c.receive_code(), Err(Error::Cancelled)));
    t.join().unwrap();
    assert_eq!(c.receive_code().unwrap().major_number, Some(9));
    m.handle.join().unwrap();
}

#[test]
fn buffered_code_wins_over_cancel() {
    let both = format!("{}{}", code_json(1), code_json(2));
    let (mut c, m) = cancel_scenario(move |mut s| {
        send(&mut s, &both);
        std::thread::sleep(Duration::from_millis(300));
    });
    assert_eq!(c.receive_code().unwrap().major_number, Some(1));
    std::thread::sleep(Duration::from_millis(50));
    c.canceller().cancel();
    // second code is fully buffered -> returned first
    assert_eq!(c.receive_code().unwrap().major_number, Some(2));
    // the flag is still set and applies to the next call
    assert!(matches!(c.receive_code(), Err(Error::Cancelled)));
    m.handle.join().unwrap();
}

#[test]
fn intercept_command_api_flush() {
    let (mut c, m) = cancel_scenario(|mut s| {
        assert_eq!(
            read_json(&mut s),
            json!({"command":"Flush","channel":"File","syncFileStreams":false,"ifExecuting":true})
        );
        send(&mut s, r#"{"success":true,"result":true}"#);
    });
    assert!(c.flush(CodeChannel::File).unwrap());
    m.handle.join().unwrap();
}
