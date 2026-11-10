//! Integration tests for Aviary's service and communication workflows.

use serde_json::{json, Value};
use std::{
    error::Error,
    net::{TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    sync::{Arc, Barrier},
    thread,
    time::Duration,
};

type TestResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start_server(database_path: &str, address: &str) -> TestResult<Server> {
    let child = Command::new(env!("CARGO_BIN_EXE_aviary"))
        .args(["serve", database_path, address])
        .stderr(Stdio::null())
        .spawn()?;
    let server = Server(child);
    for _ in 0..100 {
        if TcpStream::connect(address).is_ok() {
            return Ok(server);
        }
        thread::sleep(Duration::from_millis(20));
    }
    Err("server failed to start".into())
}

fn call(address: &str, request: Value) -> TestResult<Value> {
    call_version(address, request, 1)
}

fn call_version(address: &str, request: Value, version: u32) -> TestResult<Value> {
    let context = zmq::Context::new();
    let socket = context.socket(zmq::DEALER)?;
    socket.set_linger(0)?;
    socket.set_rcvtimeo(10_000)?;
    socket.connect(&format!("tcp://{address}"))?;
    socket.send(
        json!({"version":version,"correlation":"test","operation":request})
            .to_string()
            .as_bytes(),
        0,
    )?;
    let response = socket.recv_bytes(0)?;
    let envelope: Value = serde_json::from_slice(&response)?;
    assert_eq!(envelope.get("version"), Some(&json!(version)));
    assert_eq!(envelope.get("correlation"), Some(&json!("test")));
    Ok(envelope.get("result").ok_or("missing result")?.clone())
}

fn collect_results(tasks: Vec<thread::JoinHandle<TestResult<Value>>>) -> TestResult<Vec<Value>> {
    tasks
        .into_iter()
        .map(|task| task.join().map_err(|_| "test worker panicked")?)
        .collect()
}

#[test]
fn version_two_retrieves_near_limit_decision_and_answer() -> TestResult<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?.to_string();
    drop(listener);
    let directory =
        std::env::temp_dir().join(format!("aviary-large-record-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let database_path = directory.join("test.sqlite");
    let database = database_path.to_str().ok_or("database path is not UTF-8")?;
    let server = start_server(database, &address)?;
    let option = "x".repeat(1_048_000);
    let submit = json!({"op":"submit", "request":{"identity":"near-limit", "agent":"a", "question":"Choose", "options":[option]}});
    assert!(
        serde_json::to_vec(&json!({"version":1,"correlation":"test","operation":submit}))?.len()
            < 1_048_576
    );
    let id = call(&address, submit)?["id"].as_i64().ok_or("missing ID")?;
    let page = call_version(&address, json!({"op":"list_page"}), 2)?;
    assert_eq!(page["requests"][0]["request"]["options"][0], option);
    call(&address, json!({"op":"answer", "id":id, "index":0}))?;
    let history = json!({"op":"history_page", "kind":"decision", "id":id, "section":"decisions"});
    assert_eq!(
        call(&address, history.clone())?["error"],
        "response too large"
    );
    let page = call_version(&address, history.clone(), 2)?;
    assert_eq!(page["decisions"][0]["request"]["options"][0], option);
    assert_eq!(page["decisions"][0]["answer"]["value"], option);
    let output = Command::new(env!("CARGO_BIN_EXE_aviary"))
        .args(["call", &address, &history.to_string()])
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(result["decisions"][0]["answer"]["value"], option);
    drop(server);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn near_limit_reply_and_inherited_context_survive_restart() -> TestResult<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?.to_string();
    drop(listener);
    let directory = std::env::temp_dir().join(format!("aviary-large-reply-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("test.sqlite");
    let database = path.to_str().ok_or("database path is not UTF-8")?;
    let server = start_server(database, &address)?;
    call(
        &address,
        json!({"op":"create_channel", "channel":{"name":"team","members":["a","b"]}}),
    )?;
    let context = "c".repeat(1_048_000);
    let body = "b".repeat(1_048_000);
    let request = json!({"op":"send_message", "message":{"identity":"large-request", "channel":"team", "sender":"a", "target":{"kind":"participant","name":"b"}, "body":"respond", "expects_reply":true, "context":context}});
    assert!(
        serde_json::to_vec(&json!({"version":1,"correlation":"test","operation":request}))?.len()
            < 1_048_576
    );
    let id = call(&address, request)?["id"]
        .as_i64()
        .ok_or("missing request ID")?;
    let reply = json!({"op":"reply_message", "reply":{"identity":"large-reply", "request_id":id, "sender":"b", "body":body}});
    assert!(
        serde_json::to_vec(&json!({"version":1,"correlation":"test","operation":reply}))?.len()
            < 1_048_576
    );
    let response_id = call(&address, reply.clone())?["id"]
        .as_i64()
        .ok_or("missing response ID")?;
    let get = json!({"op":"get_message_response", "id":id});
    assert_eq!(call(&address, get.clone())?["error"], "response too large");
    let response = call_version(&address, get.clone(), 2)?;
    assert_eq!(response["response"]["id"], response_id);
    assert_eq!(response["response"]["message"]["context"], context);
    assert_eq!(response["response"]["message"]["body"], body);
    let inbox = call_version(&address, json!({"op":"inbox_page", "participant":"a"}), 2)?;
    assert_eq!(inbox["messages"][0]["message"]["body"], body);
    let history = call_version(
        &address,
        json!({"op":"history_page", "kind":"message", "id":id, "section":"messages", "offset":1}),
        2,
    )?;
    assert_eq!(history["messages"][0]["id"], response_id);
    assert_eq!(history["messages"][0]["message"]["context"], context);
    drop(server);
    let server = start_server(database, &address)?;
    assert_eq!(call(&address, reply)?["id"], response_id);
    let response = call_version(&address, get, 2)?;
    assert_eq!(response["response"]["message"]["body"], body);
    assert_eq!(response["response"]["message"]["context"], context);
    drop(server);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn durable_retry_and_single_answer() -> TestResult<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?.to_string();
    drop(listener);
    let directory = std::env::temp_dir().join(format!("aviary-test-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let database_path = directory.join("test.sqlite");
    let database = database_path.to_str().ok_or("database path is not UTF-8")?;
    let request = json!({"identity":"retry-1","agent":"agent","question":"Proceed?","options":["No","Yes"],"recommendation":1,"reason":"Ready"});
    let server = start_server(database, &address)?;
    let first = call(&address, json!({"op":"submit","request":request}))?;
    let id = first["id"].as_i64().ok_or("missing request ID")?;
    assert_eq!(
        call(&address, json!({"op":"submit","request":request}))?["id"],
        id
    );
    let mut changed = request.clone();
    changed["question"] = json!("Changed");
    assert_eq!(
        call(&address, json!({"op":"submit","request":changed}))?["error"],
        "identity conflict"
    );
    drop(server);

    let server = start_server(database, &address)?;
    let pending = call(&address, json!({"op":"list"}))?;
    assert_eq!(
        pending["requests"]
            .as_array()
            .ok_or("missing requests")?
            .len(),
        1
    );
    assert_eq!(
        call(&address, json!({"op":"answer","id":id,"index":1}))?["id"],
        id
    );
    assert_eq!(
        call(&address, json!({"op":"answer","id":id,"index":0}))?["error"],
        "request already answered"
    );
    drop(server);

    let server = start_server(database, &address)?;
    assert_eq!(
        call(&address, json!({"op":"get","id":id}))?["answer"],
        json!({"type":"suggested","index":1,"value":"Yes"})
    );
    assert_eq!(
        call(&address, json!({"op":"submit","request":request}))?["id"],
        id
    );

    let concurrent_request = json!({"identity":"concurrent", "agent":"agent", "question":"Proceed?", "options":["No","Yes"], "recommendation":1, "reason":"Ready"});
    let barrier = Arc::new(Barrier::new(8));
    let submissions = (0..8)
        .map(|_| {
            let address = address.clone();
            let request = concurrent_request.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                call(&address, json!({"op":"submit", "request":request}))
            })
        })
        .collect();
    let submission_results = collect_results(submissions)?;
    let concurrent_id = submission_results
        .first()
        .and_then(|result| result["id"].as_i64())
        .ok_or("missing concurrent request ID")?;
    assert!(submission_results
        .iter()
        .all(|result| result["id"] == concurrent_id));

    let barrier = Arc::new(Barrier::new(8));
    let answers = (0..8)
        .map(|index| {
            let address = address.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                let operation = if index % 2 == 0 {
                    json!({"op":"answer", "id":concurrent_id, "index":index / 4})
                } else {
                    json!({"op":"answer_custom", "id":concurrent_id, "value":format!("custom {index}")})
                };
                call(&address, operation)
            })
        })
        .collect();
    let answer_results = collect_results(answers)?;
    assert_eq!(
        answer_results
            .iter()
            .filter(|result| result["id"] == concurrent_id)
            .count(),
        1
    );
    assert_eq!(
        answer_results
            .iter()
            .filter(|result| result["error"] == "request already answered")
            .count(),
        7
    );
    let winner = answer_results
        .iter()
        .position(|result| result["id"] == concurrent_id)
        .ok_or("missing winning answer")?;
    let expected = if winner % 2 == 0 {
        let index = winner / 4;
        json!({"type":"suggested", "index":index, "value": if index == 0 { "No" } else { "Yes" }})
    } else {
        json!({"type":"custom", "value":format!("custom {winner}")})
    };
    assert_eq!(
        call(&address, json!({"op":"get", "id":concurrent_id}))?["answer"],
        expected
    );
    drop(server);
    let server = start_server(database, &address)?;
    assert_eq!(
        call(&address, json!({"op":"get", "id":concurrent_id}))?["answer"],
        expected
    );
    let retry = if winner % 2 == 0 {
        json!({"op":"answer", "id":concurrent_id, "index":winner / 4})
    } else {
        json!({"op":"answer_custom", "id":concurrent_id, "value":format!("custom {winner}")})
    };
    assert_eq!(call(&address, retry)?["error"], "request already answered");
    assert_eq!(
        call(&address, json!({"op":"get", "id":concurrent_id}))?["answer"],
        expected
    );
    drop(server);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

fn dealer(context: &zmq::Context, address: &str) -> TestResult<zmq::Socket> {
    let socket = context.socket(zmq::DEALER)?;
    socket.set_linger(0)?;
    socket.set_sndtimeo(5_000)?;
    socket.set_rcvtimeo(5_000)?;
    socket.connect(&format!("tcp://{address}"))?;
    Ok(socket)
}

fn send_request(socket: &zmq::Socket, correlation: &str, operation: Value) -> TestResult<()> {
    let envelope = json!({"version":1,"correlation":correlation,"operation":operation});
    socket.send(envelope.to_string().as_bytes(), 0)?;
    Ok(())
}

fn receive_reply(socket: &zmq::Socket) -> TestResult<Value> {
    Ok(serde_json::from_slice(&socket.recv_bytes(0)?)?)
}

#[test]
fn correlation_lost_reply_and_reconnect() -> TestResult<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?.to_string();
    drop(listener);
    let directory = std::env::temp_dir().join(format!("aviary-transport-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let database_path = directory.join("test.sqlite");
    let database = database_path.to_str().ok_or("database path is not UTF-8")?;
    let server = start_server(database, &address)?;
    let context = zmq::Context::new();
    let socket = dealer(&context, &address)?;

    // Multiple requests on one connection retain their distinct correlations.
    send_request(&socket, "first", json!({"op":"list"}))?;
    send_request(&socket, "second", json!({"op":"list"}))?;
    let replies = [receive_reply(&socket)?, receive_reply(&socket)?];
    let correlations: std::collections::HashSet<_> = replies
        .iter()
        .filter_map(|reply| reply.get("correlation").and_then(Value::as_str))
        .collect();
    assert_eq!(correlations, ["first", "second"].into_iter().collect());
    assert!(replies
        .iter()
        .all(|reply| reply.pointer("/result/requests").is_some()));

    // Observe the committed row independently before discarding the unread reply.
    let request = json!({"identity":"lost-reply", "agent":"agent", "question":"Proceed?", "options":["No","Yes"], "recommendation":1, "reason":"Ready"});
    send_request(
        &socket,
        "discarded",
        json!({"op":"submit","request":request}),
    )?;
    let mut observed_id = None;
    for _ in 0..100 {
        let list = call(&address, json!({"op":"list"}))?;
        observed_id = list.pointer("/requests/0/id").and_then(Value::as_i64);
        if observed_id.is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let id = observed_id.ok_or("submission did not commit")?;
    drop(socket);
    let retry = call(&address, json!({"op":"submit","request":request}))?;
    assert_eq!(retry.get("id"), Some(&json!(id)));
    assert_eq!(
        call(&address, json!({"op":"list"}))?
            .pointer("/requests")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(1)
    );

    // A DEALER that survives a service restart reconnects and can issue a new request.
    let socket = dealer(&context, &address)?;
    send_request(&socket, "before", json!({"op":"get","id":id}))?;
    assert_eq!(
        receive_reply(&socket)?.get("correlation"),
        Some(&json!("before"))
    );
    drop(server);
    let server = start_server(database, &address)?;
    send_request(&socket, "after", json!({"op":"get","id":id}))?;
    let reply = receive_reply(&socket)?;
    assert_eq!(reply.get("correlation"), Some(&json!("after")));
    assert_eq!(reply.pointer("/result/id"), Some(&json!(id)));
    drop(socket);
    drop(server);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn decision_suggestions_and_optional_recommendation() -> TestResult<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?.to_string();
    drop(listener);
    let directory = std::env::temp_dir().join(format!("aviary-options-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let database_path = directory.join("test.sqlite");
    let database = database_path.to_str().ok_or("database path is not UTF-8")?;
    let server = start_server(database, &address)?;
    let requests = [
        json!({"identity":"none","agent":"agent","question":"What next?","explanation":"Why","context":"Supporting context","options":[]}),
        json!({"identity":"two","agent":"agent","question":"Choose","options":["Yes","No"]}),
        json!({"identity":"many","agent":"agent","question":"Choose","options":["0","1","2","3","4","5","6","7","8","9","10"],"recommendation":10,"reason":"Last is best"}),
    ];
    for request in &requests {
        let first = call(&address, json!({"op":"submit","request":request}))?;
        let retry = call(&address, json!({"op":"submit","request":request}))?;
        assert!(first.get("id").and_then(Value::as_i64).is_some());
        assert_eq!(first, retry);
    }
    let invalid = [
        json!({"identity":"bad-index","agent":"agent","question":"Choose","options":[],"recommendation":0,"reason":"Bad"}),
        json!({"identity":"bad-reason","agent":"agent","question":"Choose","options":["Yes"],"recommendation":0}),
        json!({"identity":"orphan-reason","agent":"agent","question":"Choose","options":[],"reason":"Unattached"}),
    ];
    for request in invalid {
        assert_eq!(
            call(&address, json!({"op":"submit","request":request}))?.get("error"),
            Some(&json!("invalid request"))
        );
    }
    let listed = call(&address, json!({"op":"list"}))?;
    assert_eq!(
        listed.pointer("/requests/0/request/options"),
        Some(&json!([]))
    );
    assert_eq!(
        listed.pointer("/requests/0/request/explanation"),
        Some(&json!("Why"))
    );
    assert_eq!(
        listed.pointer("/requests/0/request/context"),
        Some(&json!("Supporting context"))
    );
    assert_eq!(
        listed.pointer("/requests/1/request/options"),
        Some(&json!(["Yes", "No"]))
    );
    assert_eq!(
        listed.pointer("/requests/2/request/recommendation"),
        Some(&json!(10))
    );
    assert_eq!(
        listed.pointer("/requests/2/request/reason"),
        Some(&json!("Last is best"))
    );
    let id = listed
        .pointer("/requests/0/id")
        .and_then(Value::as_i64)
        .ok_or("missing ID")?;
    let text = "Custom answer: preserve spaces  and Unicode 鳥";
    assert_eq!(
        call(&address, json!({"op":"answer_custom","id":id,"value":text}))?.get("id"),
        Some(&json!(id))
    );
    assert_eq!(
        call(&address, json!({"op":"get","id":id}))?.get("answer"),
        Some(&json!({"type":"custom","value":text}))
    );
    assert_eq!(
        call(
            &address,
            json!({"op":"answer_custom","id":id,"value":"other"})
        )?
        .get("error"),
        Some(&json!("request already answered"))
    );
    assert_eq!(
        call(
            &address,
            json!({"op":"answer_custom","id":999999,"value":"text"})
        )?
        .get("error"),
        Some(&json!("request not found"))
    );
    assert_eq!(
        call(&address, json!({"op":"answer_custom","id":id,"value":""}))?.get("error"),
        Some(&json!("invalid option"))
    );
    drop(server);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn channel_routing_and_durable_retry() -> TestResult<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?.to_string();
    drop(listener);
    let directory = std::env::temp_dir().join(format!("aviary-channel-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("test.sqlite");
    let database = path.to_str().ok_or("database path is not UTF-8")?;
    let server = start_server(database, &address)?;
    let create = json!({"op":"create_channel","channel":{"name":"team","members":["a","b","c"]}});
    let channel = call(&address, create.clone())?;
    assert_eq!(
        channel.pointer("/channel/members"),
        Some(&json!(["a", "b", "c"]))
    );
    assert_eq!(call(&address, create)?, channel);
    assert_eq!(
        call(
            &address,
            json!({"op":"create_channel","channel":{"name":"team","members":["a","b"]}})
        )?
        .get("error"),
        Some(&json!("identity conflict"))
    );
    let direct = json!({"identity":"direct","channel":"team","sender":"a","target":{"kind":"participant","name":"b"},"body":"hello","context":"work","activity":"task"});
    let group_a = json!({"identity":"group-a","channel":"team","sender":"a","target":{"kind":"group"},"body":"for team"});
    let group_b = json!({"identity":"group-b","channel":"team","sender":"b","target":{"kind":"group"},"body":"reply"});
    let id = call(&address, json!({"op":"send_message","message":direct}))?
        .get("id")
        .and_then(Value::as_i64)
        .ok_or("missing id")?;
    assert_eq!(
        call(&address, json!({"op":"send_message","message":direct}))?.get("id"),
        Some(&json!(id))
    );
    call(&address, json!({"op":"send_message","message":group_a}))?;
    call(&address, json!({"op":"send_message","message":group_b}))?;
    for (participant, expected) in [
        ("a", vec!["group-b"]),
        ("b", vec!["direct", "group-a"]),
        ("c", vec!["group-a", "group-b"]),
        ("unrelated", vec![]),
    ] {
        let inbox = call(&address, json!({"op":"inbox","participant":participant}))?;
        let messages = inbox
            .get("messages")
            .and_then(Value::as_array)
            .ok_or("missing inbox")?;
        let identities: Vec<&str> = messages
            .iter()
            .filter_map(|m| m.pointer("/message/identity").and_then(Value::as_str))
            .collect();
        assert_eq!(identities, expected);
    }
    let mut changed = direct.clone();
    changed["body"] = json!("changed");
    assert_eq!(
        call(&address, json!({"op":"send_message","message":changed}))?.get("error"),
        Some(&json!("identity conflict"))
    );
    let collision = json!({"identity":"direct","agent":"a","question":"q","options":[]});
    assert_eq!(
        call(&address, json!({"op":"submit","request":collision}))?.get("error"),
        Some(&json!("identity conflict"))
    );
    assert_eq!(call(&address, json!({"op":"send_message","message":{"identity":"invalid","channel":"team","sender":"outsider","target":{"kind":"group"},"body":"no"}}))?.get("error"), Some(&json!("invalid request")));
    drop(server);
    let server = start_server(database, &address)?;
    assert_eq!(
        call(&address, json!({"op":"send_message","message":direct}))?.get("id"),
        Some(&json!(id))
    );
    let inbox = call(&address, json!({"op":"inbox","participant":"b"}))?;
    assert_eq!(
        inbox.pointer("/messages/0/message/context"),
        Some(&json!("work"))
    );
    assert_eq!(
        inbox.pointer("/messages/0/message/activity"),
        Some(&json!("task"))
    );
    assert_eq!(
        inbox.pointer("/messages/0/message/target/name"),
        Some(&json!("b"))
    );
    drop(server);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn targeted_request_reply_wait_and_restart() -> TestResult<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?.to_string();
    drop(listener);
    let directory = std::env::temp_dir().join(format!("aviary-reply-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("test.sqlite");
    let database = path.to_str().ok_or("database path is not UTF-8")?;
    let server = start_server(database, &address)?;
    call(
        &address,
        json!({"op":"create_channel","channel":{"name":"team","members":["a","b","c"]}}),
    )?;
    let request = json!({"identity":"request-1","channel":"team","sender":"a","target":{"kind":"participant","name":"b"},"body":"Please check","expects_reply":true,"activity":"task"});
    let id = call(&address, json!({"op":"send_message","message":request}))?
        .get("id")
        .and_then(Value::as_i64)
        .ok_or("missing id")?;
    assert_eq!(
        call(&address, json!({"op":"get_message_response","id":id}))?.get("response"),
        Some(&Value::Null)
    );
    let mut waiter = Command::new(env!("CARGO_BIN_EXE_aviary"))
        .args(["wait-message", &address, &id.to_string()])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    thread::sleep(Duration::from_millis(200));
    assert!(waiter.try_wait()?.is_none());
    let reply = json!({"identity":"reply-1","request_id":id,"sender":"b","body":"Done"});
    assert_eq!(call(&address, json!({"op":"reply_message","reply":{"identity":"bad","request_id":id,"sender":"c","body":"No"}}))?.get("error"), Some(&json!("invalid request")));
    let first = call(&address, json!({"op":"reply_message","reply":reply}))?;
    assert_eq!(
        call(&address, json!({"op":"reply_message","reply":reply}))?,
        first
    );
    assert_eq!(call(&address, json!({"op":"reply_message","reply":{"identity":"second","request_id":id,"sender":"b","body":"Again"}}))?.get("error"), Some(&json!("request already answered")));
    let output = waiter.wait_with_output()?;
    assert!(output.status.success());
    let result: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(
        result.pointer("/response/message/body"),
        Some(&json!("Done"))
    );
    assert_eq!(
        result.pointer("/response/message/activity"),
        Some(&json!("task"))
    );
    let inbox = call(&address, json!({"op":"inbox","participant":"a"}))?;
    assert_eq!(
        inbox.pointer("/messages/0/message/body"),
        Some(&json!("Done"))
    );
    drop(server);
    let server = start_server(database, &address)?;
    assert_eq!(
        call(&address, json!({"op":"get_message_response","id":id}))?,
        result
    );
    assert_eq!(
        call(&address, json!({"op":"reply_message","reply":reply}))?,
        first
    );
    let restarted = Command::new(env!("CARGO_BIN_EXE_aviary"))
        .args(["wait-message", &address, &id.to_string()])
        .output()?;
    assert!(restarted.status.success());
    assert_eq!(serde_json::from_slice::<Value>(&restarted.stdout)?, result);
    drop(server);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn priority_override_and_deferred_recovery() -> TestResult<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?.to_string();
    drop(listener);
    let directory = std::env::temp_dir().join(format!("aviary-attention-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("test.sqlite");
    let database = path.to_str().ok_or("database path is not UTF-8")?;
    let server = start_server(database, &address)?;
    let mut ids = Vec::new();
    for (identity, agent, importance) in [("low", "a", 1), ("high", "b", 4), ("tie", "a", 4)] {
        let result = call(
            &address,
            json!({"op":"submit","request":{"identity":identity,"agent":agent,"question":"Choose","options":[],"importance":importance}}),
        )?;
        ids.push(
            result
                .get("id")
                .and_then(Value::as_i64)
                .ok_or("missing id")?,
        );
    }
    let queue_ids = |op: &str| -> TestResult<Vec<i64>> {
        let result = call(&address, json!({"op":op}))?;
        Ok(result
            .get("requests")
            .and_then(Value::as_array)
            .ok_or("missing requests")?
            .iter()
            .filter_map(|item| item.get("id").and_then(Value::as_i64))
            .collect())
    };
    let low = *ids.first().ok_or("missing low")?;
    let high = *ids.get(1).ok_or("missing high")?;
    let tie = *ids.get(2).ok_or("missing tie")?;
    assert_eq!(queue_ids("list")?, vec![high, tie, low]);
    call(
        &address,
        json!({"op":"set_importance","id":low,"importance":5}),
    )?;
    assert_eq!(queue_ids("list")?, vec![low, high, tie]);
    call(
        &address,
        json!({"op":"set_deferred","id":low,"deferred":true}),
    )?;
    assert_eq!(queue_ids("list")?, vec![high, tie]);
    assert_eq!(queue_ids("list_deferred")?, vec![low]);
    drop(server);
    let server = start_server(database, &address)?;
    assert_eq!(queue_ids("list_deferred")?, vec![low]);
    call(
        &address,
        json!({"op":"set_deferred","id":low,"deferred":false}),
    )?;
    assert_eq!(queue_ids("list")?, vec![low, high, tie]);
    assert_eq!(
        call(
            &address,
            json!({"op":"set_importance","id":low,"importance":6})
        )?
        .get("error"),
        Some(&json!("invalid request"))
    );
    drop(server);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn completion_notice_retry_and_dismissal_recover() -> TestResult<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?.to_string();
    drop(listener);
    let directory = std::env::temp_dir().join(format!("aviary-notice-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("test.sqlite");
    let database = path.to_str().ok_or("database path is not UTF-8")?;
    let server = start_server(database, &address)?;
    let notice = json!({"identity":"notice-1","agent":"worker","work":"Run checks","result":"All passed","activity":"task-1"});
    let first = call(&address, json!({"op":"submit_notice","notice":notice}))?;
    let id = first
        .get("id")
        .and_then(Value::as_i64)
        .ok_or("missing id")?;
    assert_eq!(
        call(&address, json!({"op":"submit_notice","notice":notice}))?,
        first
    );
    let mut changed = notice.clone();
    changed["result"] = json!("Failed");
    assert_eq!(
        call(&address, json!({"op":"submit_notice","notice":changed}))?.get("error"),
        Some(&json!("identity conflict"))
    );
    let listed = call(&address, json!({"op":"list_notices"}))?;
    assert_eq!(
        listed.pointer("/notices/0/notice/work"),
        Some(&json!("Run checks"))
    );
    assert_eq!(
        listed.pointer("/notices/0/notice/result"),
        Some(&json!("All passed"))
    );
    drop(server);
    let server = start_server(database, &address)?;
    assert_eq!(call(&address, json!({"op":"list_notices"}))?, listed);
    call(&address, json!({"op":"dismiss_notice","id":id}))?;
    assert_eq!(
        call(&address, json!({"op":"list_notices"}))?.get("notices"),
        Some(&json!([]))
    );
    drop(server);
    let server = start_server(database, &address)?;
    let history = call(
        &address,
        json!({"op":"list_notices","include_dismissed":true}),
    )?;
    assert_eq!(history.pointer("/notices/0/dismissed"), Some(&json!(true)));
    assert_eq!(
        call(&address, json!({"op":"submit_notice","notice":notice}))?,
        first
    );
    assert_eq!(
        call(&address, json!({"op":"list_notices"}))?.get("notices"),
        Some(&json!([]))
    );
    drop(server);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

fn assert_history_page_parity(address: &str, kind: &str, id: i64) -> TestResult<()> {
    let history = call(address, json!({"op":"history","kind":kind,"id":id}))?;
    for section in ["decisions", "messages", "outcomes"] {
        let expected = history
            .get(section)
            .and_then(Value::as_array)
            .ok_or("missing section")?;
        for offset in 0..=expected.len() + 1 {
            let page = call(
                address,
                json!({"op":"history_page","kind":kind,"id":id,"section":section,"offset":offset}),
            )?;
            let tail: Vec<Value> = expected.iter().skip(offset).cloned().collect();
            assert_eq!(
                page.get(section),
                Some(&json!(tail)),
                "{kind} {id} {section} {offset}"
            );
            assert_eq!(page.get("next_offset"), Some(&Value::Null));
            assert_eq!(page.get("section"), Some(&json!(section)));
            assert_eq!(page.get("id"), Some(&json!(id)));
        }
        let beyond = call(
            address,
            json!({"op":"history_page","kind":kind,"id":id,"section":section,"offset":usize::MAX}),
        )?;
        assert_eq!(beyond.get(section), Some(&json!([])));
        assert_eq!(beyond.get("next_offset"), Some(&Value::Null));
    }
    Ok(())
}

#[test]
fn request_history_links_decisions_messages_and_outcomes() -> TestResult<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?.to_string();
    drop(listener);
    let directory = std::env::temp_dir().join(format!("aviary-history-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("test.sqlite");
    let database = path.to_str().ok_or("database path is not UTF-8")?;
    let server = start_server(database, &address)?;
    call(
        &address,
        json!({"op":"create_channel","channel":{"name":"team","members":["a","b"]}}),
    )?;
    let decision = json!({"identity":"hist-decision","agent":"a","question":"Approve?","options":["Yes"],"recommendation":0,"reason":"Ready","activity":"task-x"});
    let decision_id = call(&address, json!({"op":"submit","request":decision}))?
        .get("id")
        .and_then(Value::as_i64)
        .ok_or("missing decision")?;
    call(&address, json!({"op":"answer","id":decision_id,"index":0}))?;
    let message = json!({"identity":"hist-request","channel":"team","sender":"a","target":{"kind":"participant","name":"b"},"body":"Check","expects_reply":true,"activity":"task-x"});
    let message_id = call(&address, json!({"op":"send_message","message":message}))?
        .get("id")
        .and_then(Value::as_i64)
        .ok_or("missing message")?;
    let reply_id = call(&address, json!({"op":"reply_message","reply":{"identity":"hist-reply","request_id":message_id,"sender":"b","body":"Checked"}}))?.get("id").and_then(Value::as_i64).ok_or("missing reply")?;
    call(
        &address,
        json!({"op":"submit_notice","notice":{"identity":"hist-outcome","agent":"b","work":"Check","result":"Passed","activity":"task-x"}}),
    )?;
    call(
        &address,
        json!({"op":"submit_notice","notice":{"identity":"other-outcome","agent":"b","work":"Other","result":"Ignored","activity":"task-y"}}),
    )?;
    let history = call(
        &address,
        json!({"op":"history","kind":"decision","id":decision_id}),
    )?;
    assert_eq!(history.pointer("/activity"), Some(&json!("task-x")));
    assert_eq!(
        history.pointer("/decisions/0/answer/value"),
        Some(&json!("Yes"))
    );
    assert_eq!(history.pointer("/messages/0/id"), Some(&json!(message_id)));
    assert_eq!(history.pointer("/messages/1/id"), Some(&json!(reply_id)));
    assert_eq!(
        history.pointer("/messages/1/reply_to"),
        Some(&json!(message_id))
    );
    assert_eq!(
        history.pointer("/outcomes/0/notice/result"),
        Some(&json!("Passed"))
    );
    assert_eq!(
        history
            .get("outcomes")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(1)
    );
    let message_history = call(
        &address,
        json!({"op":"history","kind":"message","id":message_id}),
    )?;
    assert_eq!(message_history.get("decisions"), history.get("decisions"));
    assert_eq!(message_history.get("messages"), history.get("messages"));
    drop(server);
    let server = start_server(database, &address)?;
    assert_eq!(
        call(
            &address,
            json!({"op":"history","kind":"decision","id":decision_id})
        )?,
        history
    );
    let plain = json!({"identity":"plain-request","channel":"team","sender":"a","target":{"kind":"participant","name":"b"},"body":"Check","expects_reply":true});
    let plain_id = call(&address, json!({"op":"send_message","message":plain}))?
        .get("id")
        .and_then(Value::as_i64)
        .ok_or("missing request")?;
    let plain_reply = call(&address, json!({"op":"reply_message","reply":{"identity":"plain-reply","request_id":plain_id,"sender":"b","body":"Done"}}))?.get("id").and_then(Value::as_i64).ok_or("missing reply")?;
    for root in [plain_id, plain_reply] {
        let plain_history = call(&address, json!({"op":"history","kind":"message","id":root}))?;
        assert_eq!(
            plain_history
                .get("messages")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(2)
        );
        assert_eq!(
            plain_history.pointer("/messages/1/reply_to"),
            Some(&json!(plain_id))
        );
        assert_eq!(plain_history.get("outcomes"), Some(&json!([])));
    }
    for (kind, id) in [
        ("decision", decision_id),
        ("message", message_id),
        ("message", reply_id),
        ("message", plain_id),
        ("message", plain_reply),
    ] {
        assert_history_page_parity(&address, kind, id)?;
    }
    assert_eq!(call(&address, json!({"op":"history_page","kind":"decision","id":999999,"section":"messages","offset":usize::MAX}))?.get("error"), Some(&json!("request not found")));
    drop(server);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn unavailable_service_reports_timeout() -> TestResult<()> {
    // Hold the port without a ZeroMQ service, so another process cannot bind it.
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?.to_string();
    let start = std::time::Instant::now();
    let result = Command::new(env!("CARGO_BIN_EXE_aviary"))
        .args(["call", &address, r#"{"op":"list"}"#])
        .output()?;
    let elapsed = start.elapsed();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("operation timed out"));
    assert!(
        elapsed >= Duration::from_secs(8) && elapsed < Duration::from_secs(25),
        "elapsed {elapsed:?}"
    );
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn local_send_queue_applies_backpressure_and_terminates() -> TestResult<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?.to_string();
    drop(listener);
    let directory = std::env::temp_dir().join(format!("aviary-pressure-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("test.sqlite");
    let database = path.to_str().ok_or("database path is not UTF-8")?;
    let server = start_server(database, &address)?;
    let context = zmq::Context::new();
    let socket = context.socket(zmq::DEALER)?;
    socket.set_linger(0)?;
    socket.set_sndhwm(1)?;
    socket.set_immediate(true)?;
    socket.set_sndtimeo(5_000)?;
    socket.set_rcvtimeo(5_000)?;
    socket.connect(&format!("tcp://{address}"))?;
    send_request(&socket, "ready", json!({"op":"list"}))?;
    assert_eq!(
        receive_reply(&socket)?.get("correlation"),
        Some(&json!("ready"))
    );
    assert!(Command::new("kill")
        .args(["-STOP", &server.0.id().to_string()])
        .status()?
        .success());
    // A non-reading stopped peer eventually fills the local per-peer queue.
    let payload = vec![b'x'; 512 * 1024];
    let mut accepted = 0;
    let mut backpressure = false;
    for _ in 0..2048 {
        match socket.send(payload.as_slice(), zmq::DONTWAIT) {
            Ok(()) => accepted += 1,
            Err(zmq::Error::EAGAIN) => {
                backpressure = true;
                break;
            }
            Err(error) => return Err(error.into()),
        }
    }
    assert!(
        accepted > 0 && backpressure,
        "accepted={accepted}, backpressure={backpressure}"
    );
    let started = std::time::Instant::now();
    drop(socket);
    drop(server);
    assert!(started.elapsed() < Duration::from_secs(5));
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn queue_pages_cross_frame_limit() -> TestResult<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?.to_string();
    drop(listener);
    let directory = std::env::temp_dir().join(format!("aviary-pages-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("test.sqlite");
    let database = path.to_str().ok_or("database path is not UTF-8")?;
    let server = start_server(database, &address)?;
    let large = "x".repeat(350_000);
    for i in 0..4 {
        let request = json!({"identity":format!("page-request-{i}"),"agent":"a","question":"Choose","context":large,"options":[],"importance":i});
        assert!(call(&address, json!({"op":"submit","request":request}))?
            .get("id")
            .is_some());
        let notice =
            json!({"identity":format!("page-notice-{i}"),"agent":"a","work":"Work","result":large});
        assert!(
            call(&address, json!({"op":"submit_notice","notice":notice}))?
                .get("id")
                .is_some()
        );
    }
    assert_eq!(
        call(&address, json!({"op":"list"}))?.get("error"),
        Some(&json!("response too large"))
    );
    for (operation, field) in [("list_page", "requests"), ("notices_page", "notices")] {
        let mut offset = 0u64;
        let mut seen = Vec::new();
        let mut pages = 0;
        loop {
            let response = call(&address, json!({"op":operation,"offset":offset}))?;
            let items = response
                .get(field)
                .and_then(Value::as_array)
                .ok_or("missing page")?;
            assert!(!items.is_empty());
            seen.extend(
                items
                    .iter()
                    .filter_map(|item| item.get("id").and_then(Value::as_i64)),
            );
            pages += 1;
            match response.get("next_offset").and_then(Value::as_u64) {
                Some(next) => {
                    assert!(next > offset);
                    offset = next;
                }
                None => break,
            }
        }
        assert_eq!(seen.len(), 4);
        assert!(pages > 1);
        let unique: std::collections::HashSet<_> = seen.into_iter().collect();
        assert_eq!(unique.len(), 4);
    }
    drop(server);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn inbox_and_history_pages_cross_frame_limit() -> TestResult<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?.to_string();
    drop(listener);
    let directory =
        std::env::temp_dir().join(format!("aviary-history-pages-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("test.sqlite");
    let database = path.to_str().ok_or("database path is not UTF-8")?;
    let server = start_server(database, &address)?;
    call(
        &address,
        json!({"op":"create_channel","channel":{"name":"team","members":["a","b"]}}),
    )?;
    let large = "x".repeat(350_000);
    let mut root = None;
    for i in 0..4 {
        let decision = json!({"identity":format!("history-decision-{i}"),"agent":"a","question":"Choose","context":large,"options":[],"activity":"large-history"});
        let result = call(&address, json!({"op":"submit","request":decision}))?;
        if root.is_none() {
            root = result.get("id").and_then(Value::as_i64);
        }
        let message = json!({"identity":format!("history-message-{i}"),"channel":"team","sender":"a","target":{"kind":"participant","name":"b"},"body":large,"activity":"large-history"});
        assert!(
            call(&address, json!({"op":"send_message","message":message}))?
                .get("id")
                .is_some()
        );
        let notice = json!({"identity":format!("history-notice-{i}"),"agent":"a","work":"Work","result":large,"activity":"large-history"});
        assert!(
            call(&address, json!({"op":"submit_notice","notice":notice}))?
                .get("id")
                .is_some()
        );
    }
    let id = root.ok_or("missing root")?;
    assert_eq!(
        call(&address, json!({"op":"inbox","participant":"b"}))?.get("error"),
        Some(&json!("response too large"))
    );
    assert_eq!(
        call(&address, json!({"op":"history","kind":"decision","id":id}))?.get("error"),
        Some(&json!("response too large"))
    );
    for section in ["inbox", "decisions", "messages", "outcomes"] {
        let mut offset = 0u64;
        let mut seen = Vec::new();
        let mut pages = 0;
        loop {
            let query = if section == "inbox" {
                json!({"op":"inbox_page","participant":"b","offset":offset})
            } else {
                json!({"op":"history_page","kind":"decision","id":id,"section":section,"offset":offset})
            };
            let response = call(&address, query)?;
            let field = if section == "inbox" {
                "messages"
            } else {
                section
            };
            let items = response
                .get(field)
                .and_then(Value::as_array)
                .ok_or("missing page")?;
            seen.extend(
                items
                    .iter()
                    .filter_map(|item| item.get("id").and_then(Value::as_i64)),
            );
            pages += 1;
            match response.get("next_offset").and_then(Value::as_u64) {
                Some(next) => {
                    assert!(next > offset);
                    offset = next;
                }
                None => break,
            }
        }
        assert_eq!(seen.len(), 4, "{section}");
        assert!(pages > 1, "{section}");
        assert_eq!(
            seen.into_iter()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            4
        );
    }
    assert_eq!(
        call(
            &address,
            json!({"op":"inbox_page","participant":"unrelated"})
        )?
        .get("messages"),
        Some(&json!([]))
    );
    drop(server);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn failed_message_and_reply_writes_leave_no_partial_delivery() -> TestResult<()> {
    use sqlx::{Connection, SqliteConnection};

    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?.to_string();
    drop(listener);
    let directory =
        std::env::temp_dir().join(format!("aviary-write-failure-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("test.sqlite");
    let database = path.to_str().ok_or("database path is not UTF-8")?;
    let server = start_server(database, &address)?;
    call(
        &address,
        json!({"op":"create_channel", "channel":{"name":"team", "members":["a","b"]}}),
    )?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let mut connection =
        runtime.block_on(SqliteConnection::connect(&format!("sqlite://{database}")))?;
    runtime.block_on(sqlx::query("CREATE TRIGGER fail_delivery BEFORE INSERT ON message_recipients BEGIN SELECT RAISE(ABORT, 'injected delivery failure'); END").execute(&mut connection))?;
    let send = json!({"op":"send_message", "message":{"identity":"fault-request", "channel":"team", "sender":"a", "target":{"kind":"participant","name":"b"}, "body":"Review", "expects_reply":true}});
    let failed = call(&address, send.clone())?;
    assert!(failed.get("error").is_some() && failed.get("id").is_none());
    let identities: i64 = runtime.block_on(
        sqlx::query_scalar("SELECT COUNT(*) FROM submission_keys WHERE identity = 'fault-request'")
            .fetch_one(&mut connection),
    )?;
    let messages: i64 = runtime.block_on(
        sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE identity = 'fault-request'")
            .fetch_one(&mut connection),
    )?;
    assert_eq!((identities, messages), (0, 0));
    assert_eq!(
        call(&address, json!({"op":"inbox", "participant":"b"}))?["messages"],
        json!([])
    );
    runtime.block_on(sqlx::query("DROP TRIGGER fail_delivery").execute(&mut connection))?;
    let request_id = call(&address, send.clone())?["id"]
        .as_i64()
        .ok_or("missing request ID")?;
    assert_eq!(call(&address, send)?["id"], request_id);

    runtime.block_on(sqlx::query("CREATE TRIGGER fail_response BEFORE INSERT ON message_responses BEGIN SELECT RAISE(ABORT, 'injected response failure'); END").execute(&mut connection))?;
    let reply = json!({"op":"reply_message", "reply":{"identity":"fault-reply", "request_id":request_id, "sender":"b", "body":"Done"}});
    let failed = call(&address, reply.clone())?;
    assert!(failed.get("error").is_some() && failed.get("id").is_none());
    let identities: i64 = runtime.block_on(
        sqlx::query_scalar("SELECT COUNT(*) FROM submission_keys WHERE identity = 'fault-reply'")
            .fetch_one(&mut connection),
    )?;
    let messages: i64 = runtime.block_on(
        sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE identity = 'fault-reply'")
            .fetch_one(&mut connection),
    )?;
    assert_eq!((identities, messages), (0, 0));
    assert_eq!(
        call(&address, json!({"op":"inbox", "participant":"a"}))?["messages"],
        json!([])
    );
    assert!(call(
        &address,
        json!({"op":"get_message_response", "id":request_id})
    )?["response"]
        .is_null());
    runtime.block_on(sqlx::query("DROP TRIGGER fail_response").execute(&mut connection))?;
    drop(connection);
    drop(server);
    let server = start_server(database, &address)?;
    let reply_id = call(&address, reply.clone())?["id"]
        .as_i64()
        .ok_or("missing reply ID")?;
    assert_eq!(call(&address, reply)?["id"], reply_id);
    let response = call(
        &address,
        json!({"op":"get_message_response", "id":request_id}),
    )?;
    assert_eq!(response["response"]["id"], reply_id);
    assert_eq!(response["response"]["message"]["body"], "Done");
    assert_eq!(
        call(&address, json!({"op":"inbox", "participant":"a"}))?["messages"]
            .as_array()
            .ok_or("missing inbox")?
            .len(),
        1
    );
    drop(server);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn recipient_retrieves_messages_accepted_while_disconnected() -> TestResult<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?.to_string();
    drop(listener);
    let directory =
        std::env::temp_dir().join(format!("aviary-recipient-reconnect-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("test.sqlite");
    let database = path.to_str().ok_or("database path is not UTF-8")?;
    let server = start_server(database, &address)?;
    call(
        &address,
        json!({"op":"create_channel","channel":{"name":"team","members":["a","b"]}}),
    )?;
    let context = zmq::Context::new();
    let recipient = dealer(&context, &address)?;
    send_request(
        &recipient,
        "before",
        json!({"op":"inbox_page","participant":"b"}),
    )?;
    assert_eq!(
        receive_reply(&recipient)?.pointer("/result/messages"),
        Some(&json!([]))
    );
    recipient.disconnect(&format!("tcp://{address}"))?;
    let message = json!({"identity":"while-disconnected","channel":"team","sender":"a","target":{"kind":"participant","name":"b"},"body":"accepted while away"});
    let accepted = call(&address, json!({"op":"send_message","message":message}))?;
    let id = accepted
        .get("id")
        .and_then(Value::as_i64)
        .ok_or("message was not accepted")?;
    recipient.connect(&format!("tcp://{address}"))?;
    send_request(
        &recipient,
        "after",
        json!({"op":"inbox_page","participant":"b"}),
    )?;
    let recovered = receive_reply(&recipient)?;
    assert_eq!(recovered.get("correlation"), Some(&json!("after")));
    assert_eq!(recovered.pointer("/result/messages/0/id"), Some(&json!(id)));
    assert_eq!(
        recovered.pointer("/result/messages/0/message/body"),
        Some(&json!("accepted while away"))
    );
    assert_eq!(
        call(&address, json!({"op":"inbox_page","participant":"a"}))?.get("messages"),
        Some(&json!([]))
    );
    drop(recipient);
    drop(server);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
#[cfg(unix)]
fn signal_shutdown_preserves_acknowledged_submissions() -> TestResult<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?.to_string();
    drop(listener);
    let directory = std::env::temp_dir().join(format!("aviary-shutdown-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("test.sqlite");
    let database = path.to_str().ok_or("database path is not UTF-8")?;
    let request =
        json!({"identity":"shutdown-retry","agent":"agent","question":"Proceed?","options":[]});
    let mut server = start_server(database, &address)?;
    let first = call(&address, json!({"op":"submit","request":request}))?;
    let id = first
        .get("id")
        .and_then(Value::as_i64)
        .ok_or("missing ID")?;
    for signal in ["-TERM", "-INT"] {
        let status = Command::new("kill")
            .args([signal, &server.0.id().to_string()])
            .status()?;
        assert!(status.success());
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = server.0.try_wait()? {
                assert!(status.success(), "{signal}: {status}");
                break;
            }
            if std::time::Instant::now() >= deadline {
                return Err(format!("{signal}: service failed to stop").into());
            }
            thread::sleep(Duration::from_millis(20));
        }
        drop(server);
        server = start_server(database, &address)?;
        assert_eq!(
            call(&address, json!({"op":"submit","request":request}))?.get("id"),
            Some(&json!(id))
        );
        assert_eq!(
            call(&address, json!({"op":"get","id":id}))?.get("id"),
            Some(&json!(id))
        );
    }
    drop(server);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
#[cfg(unix)]
fn signal_shutdown_waits_for_in_flight_write() -> TestResult<()> {
    use sqlx::{Connection, SqliteConnection};
    use std::{
        io::{BufRead, BufReader},
        sync::mpsc,
    };

    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?.to_string();
    drop(listener);
    let directory =
        std::env::temp_dir().join(format!("aviary-shutdown-inflight-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("test.sqlite");
    let database = path.to_str().ok_or("database path is not UTF-8")?;
    let child = Command::new(env!("CARGO_BIN_EXE_aviary"))
        .args(["serve", database, &address])
        .env("RUST_LOG", "aviary=debug")
        .stderr(Stdio::piped())
        .spawn()?;
    let mut server = Server(child);
    let stderr = server.0.stderr.take().ok_or("missing service stderr")?;
    let (log_tx, log_rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        for line in BufReader::new(stderr).lines() {
            let Ok(line) = line else { break };
            if log_tx.send(line).is_err() {
                break;
            }
        }
    });
    let wait_for = |needle: &str| -> TestResult<()> {
        let deadline = std::time::Instant::now() + Duration::from_secs(4);
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            let line = log_rx.recv_timeout(remaining)?;
            if line.contains(needle) {
                return Ok(());
            }
        }
    };
    wait_for("service listening")?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let mut lock = runtime.block_on(SqliteConnection::connect(&format!("sqlite://{database}")))?;
    runtime.block_on(sqlx::query("BEGIN IMMEDIATE").execute(&mut lock))?;
    let context = zmq::Context::new();
    let socket = dealer(&context, &address)?;
    let request =
        json!({"identity":"inflight-shutdown","agent":"agent","question":"Proceed?","options":[]});
    send_request(
        &socket,
        "inflight",
        json!({"op":"submit","request":request}),
    )?;
    wait_for("processing request")?;
    assert!(Command::new("kill")
        .args(["-TERM", &server.0.id().to_string()])
        .status()?
        .success());
    wait_for("shutdown requested")?;
    assert!(
        server.0.try_wait()?.is_none(),
        "service exited while the write was locked"
    );
    runtime.block_on(sqlx::query("COMMIT").execute(&mut lock))?;
    drop(lock);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = server.0.try_wait()? {
            assert!(status.success(), "{status}");
            break;
        }
        if std::time::Instant::now() >= deadline {
            return Err("service failed to stop".into());
        }
        thread::sleep(Duration::from_millis(20));
    }
    drop(socket);
    drop(server);
    reader.join().map_err(|_| "log reader panicked")?;
    let server = start_server(database, &address)?;
    let before_retry = call(&address, json!({"op":"list"}))?;
    let original_id = before_retry
        .pointer("/requests/0/id")
        .and_then(Value::as_i64)
        .ok_or("in-flight submission did not commit")?;
    let retry = call(&address, json!({"op":"submit","request":request}))?;
    let id = retry
        .get("id")
        .and_then(Value::as_i64)
        .ok_or("missing ID")?;
    assert_eq!(id, original_id);
    let list = call(&address, json!({"op":"list"}))?;
    let requests = list
        .get("requests")
        .and_then(Value::as_array)
        .ok_or("missing requests")?;
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests.first().and_then(|item| item.get("id")),
        Some(&json!(id))
    );
    drop(server);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn message_and_reply_lost_ack_retry_after_restart() -> TestResult<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?.to_string();
    drop(listener);
    let directory =
        std::env::temp_dir().join(format!("aviary-message-lost-ack-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("test.sqlite");
    let database = path.to_str().ok_or("database path is not UTF-8")?;
    let server = start_server(database, &address)?;
    call(
        &address,
        json!({"op":"create_channel","channel":{"name":"team","members":["a","b"]}}),
    )?;
    let context = zmq::Context::new();
    let socket = dealer(&context, &address)?;
    let message = json!({"identity":"message-lost-ack","channel":"team","sender":"a","target":{"kind":"participant","name":"b"},"body":"Question","expects_reply":true,"activity":"work"});
    send_request(
        &socket,
        "message",
        json!({"op":"send_message","message":message}),
    )?;
    let mut message_id = None;
    for _ in 0..100 {
        let inbox = call(&address, json!({"op":"inbox","participant":"b"}))?;
        message_id = inbox.pointer("/messages/0/id").and_then(Value::as_i64);
        if message_id.is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let message_id = message_id.ok_or("message did not commit")?;
    // Discard the unread application reply and abruptly stop the service.
    drop(socket);
    drop(server);
    let server = start_server(database, &address)?;
    assert_eq!(
        call(&address, json!({"op":"send_message","message":message}))?.get("id"),
        Some(&json!(message_id))
    );
    let mut changed_message = message.clone();
    changed_message["body"] = json!("Changed");
    assert_eq!(
        call(
            &address,
            json!({"op":"send_message","message":changed_message})
        )?
        .get("error"),
        Some(&json!("identity conflict"))
    );
    let socket = dealer(&context, &address)?;
    let reply = json!({"identity":"reply-lost-ack","request_id":message_id,"sender":"b","body":"Original response"});
    send_request(
        &socket,
        "reply",
        json!({"op":"reply_message","reply":reply}),
    )?;
    let mut response = None;
    for _ in 0..100 {
        let result = call(
            &address,
            json!({"op":"get_message_response","id":message_id}),
        )?;
        if !result.get("response").unwrap_or(&Value::Null).is_null() {
            response = Some(result);
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let response = response.ok_or("response did not commit")?;
    let reply_id = response
        .pointer("/response/id")
        .and_then(Value::as_i64)
        .ok_or("missing response id")?;
    drop(socket);
    drop(server);
    let server = start_server(database, &address)?;
    assert_eq!(
        call(
            &address,
            json!({"op":"get_message_response","id":message_id})
        )?,
        response
    );
    assert_eq!(
        call(&address, json!({"op":"reply_message","reply":reply}))?.get("id"),
        Some(&json!(reply_id))
    );
    let mut changed_reply = reply.clone();
    changed_reply["body"] = json!("Changed response");
    assert_eq!(
        call(
            &address,
            json!({"op":"reply_message","reply":changed_reply})
        )?
        .get("error"),
        Some(&json!("identity conflict"))
    );
    let another_reply = json!({"identity":"different-reply","request_id":message_id,"sender":"b","body":"Another response"});
    assert_eq!(
        call(
            &address,
            json!({"op":"reply_message","reply":another_reply})
        )?
        .get("error"),
        Some(&json!("request already answered"))
    );
    for (participant, expected_id, expected_body) in [
        ("a", reply_id, "Original response"),
        ("b", message_id, "Question"),
    ] {
        let inbox = call(&address, json!({"op":"inbox","participant":participant}))?;
        let messages = inbox
            .get("messages")
            .and_then(Value::as_array)
            .ok_or("missing inbox")?;
        assert_eq!(messages.len(), 1);
        assert_eq!(
            messages.first().and_then(|item| item.get("id")),
            Some(&json!(expected_id))
        );
        assert_eq!(
            messages
                .first()
                .and_then(|item| item.pointer("/message/body")),
            Some(&json!(expected_body))
        );
    }
    drop(server);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn streamed_pages_match_unpaged_order_and_offsets() -> TestResult<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?.to_string();
    drop(listener);
    let directory =
        std::env::temp_dir().join(format!("aviary-stream-pages-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("test.sqlite");
    let database = path.to_str().ok_or("database path is not UTF-8")?;
    let server = start_server(database, &address)?;
    call(
        &address,
        json!({"op":"create_channel","channel":{"name":"team","members":["a","b"]}}),
    )?;
    let mut ids = Vec::new();
    for (i, priority) in [0, 3, 3, 1].into_iter().enumerate() {
        let result = call(
            &address,
            json!({"op":"submit","request":{"identity":format!("stream-decision-{i}"),"agent":"a","question":"Choose","options":[],"importance":priority}}),
        )?;
        ids.push(
            result
                .get("id")
                .and_then(Value::as_i64)
                .ok_or("missing id")?,
        );
        call(
            &address,
            json!({"op":"send_message","message":{"identity":format!("stream-message-{i}"),"channel":"team","sender":"a","target":{"kind":"participant","name":"b"},"body":format!("message-{i}")}}),
        )?;
        call(
            &address,
            json!({"op":"submit_notice","notice":{"identity":format!("stream-notice-{i}"),"agent":"a","work":"Work","result":"Done"}}),
        )?;
    }
    let first = *ids.first().ok_or("missing first id")?;
    let last = *ids.last().ok_or("missing last id")?;
    call(
        &address,
        json!({"op":"set_importance","id":first,"importance":5}),
    )?;
    call(
        &address,
        json!({"op":"set_deferred","id":last,"deferred":true}),
    )?;
    for (whole, page, field) in [
        (json!({"op":"list"}), json!({"op":"list_page"}), "requests"),
        (
            json!({"op":"list_deferred"}),
            json!({"op":"list_page","deferred":true}),
            "requests",
        ),
        (
            json!({"op":"list_notices"}),
            json!({"op":"notices_page"}),
            "notices",
        ),
        (
            json!({"op":"inbox","participant":"b"}),
            json!({"op":"inbox_page","participant":"b"}),
            "messages",
        ),
    ] {
        let unpaged = call(&address, whole)?;
        let expected = unpaged
            .get(field)
            .and_then(Value::as_array)
            .ok_or("missing list")?;
        for offset in 0..=expected.len() + 1 {
            let mut query = page.clone();
            query["offset"] = json!(offset);
            let actual = call(&address, query)?;
            let expected_tail: Vec<Value> = expected.iter().skip(offset).cloned().collect();
            assert_eq!(
                actual.get(field),
                Some(&json!(expected_tail)),
                "{field} offset {offset}"
            );
            assert_eq!(actual.get("next_offset"), Some(&Value::Null));
        }
    }
    drop(server);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn malformed_multipart_is_discarded_without_losing_next_request() -> TestResult<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?.to_string();
    drop(listener);
    let directory = std::env::temp_dir().join(format!("aviary-multipart-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("test.sqlite");
    let database = path.to_str().ok_or("database path is not UTF-8")?;
    let server = start_server(database, &address)?;
    let context = zmq::Context::new();
    let socket = dealer(&context, &address)?;
    let unexpected = json!({"version":1,"correlation":"malformed","operation":{"op":"submit","request":{"identity":"must-not-commit","agent":"a","question":"Never accept","options":[]}}}).to_string().into_bytes();
    let mut frames = vec![unexpected];
    frames.extend((0..96).map(|_| vec![b'x'; 65_536]));
    socket.send_multipart(frames, 0)?;
    send_request(&socket, "valid", json!({"op":"list"}))?;
    let response = receive_reply(&socket)?;
    assert_eq!(response.get("correlation"), Some(&json!("valid")));
    assert_eq!(response.pointer("/result/requests"), Some(&json!([])));
    let request =
        json!({"identity":"after-malformed","agent":"a","question":"Continue?","options":[]});
    assert!(call(&address, json!({"op":"submit","request":request}))?
        .get("id")
        .is_some());
    drop(socket);
    drop(server);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn client_rejects_multipart_response() -> TestResult<()> {
    let context = zmq::Context::new();
    let socket = context.socket(zmq::ROUTER)?;
    socket.set_linger(0)?;
    socket.set_rcvtimeo(5_000)?;
    socket.bind("tcp://127.0.0.1:*")?;
    let endpoint = socket
        .get_last_endpoint()?
        .map_err(|_| "non-UTF-8 endpoint")?;
    let address = endpoint
        .strip_prefix("tcp://")
        .ok_or("unexpected endpoint")?;
    let child = Command::new(env!("CARGO_BIN_EXE_aviary"))
        .args(["call", address, r#"{"op":"list"}"#])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let request = socket.recv_multipart(0)?;
    let identity = request.first().ok_or("missing identity")?;
    let response = json!({"version":2,"correlation":"1","result":{"requests":[]}}).to_string();
    socket.send_multipart([identity.as_slice(), response.as_bytes(), b"unexpected"], 0)?;
    let output = child.wait_with_output()?;
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("message too large or incomplete"));
    Ok(())
}

#[test]
fn slow_peer_backpressure_does_not_block_other_clients() -> TestResult<()> {
    use sqlx::{Connection, SqliteConnection};
    use std::{
        io::{BufRead, BufReader},
        sync::mpsc,
    };
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?.to_string();
    drop(listener);
    let directory =
        std::env::temp_dir().join(format!("aviary-router-pressure-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("test.sqlite");
    let database = path.to_str().ok_or("database path is not UTF-8")?;
    let child = Command::new(env!("CARGO_BIN_EXE_aviary"))
        .args(["serve", database, &address])
        .env("RUST_LOG", "aviary=debug")
        .stderr(Stdio::piped())
        .spawn()?;
    let mut server = Server(child);
    let stderr = server.0.stderr.take().ok_or("missing service stderr")?;
    let (tx, rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        for line in BufReader::new(stderr)
            .lines()
            .map_while(std::result::Result::ok)
        {
            if (line.contains("service listening")
                || line.contains("reply dropped: peer queue full"))
                && tx.send(line).is_err()
            {
                break;
            }
        }
    });
    let wait_for = |needle: &str, timeout: Duration| -> TestResult<()> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let line =
                rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))?;
            if line.contains(needle) {
                return Ok(());
            }
        }
    };
    wait_for("service listening", Duration::from_secs(5))?;
    let large = json!({"identity":"pressure-large","agent":"a","question":"Large","options":[],"context":"x".repeat(700_000)});
    call(&address, json!({"op":"submit","request":large}))?;
    let context = zmq::Context::new();
    let slow = context.socket(zmq::DEALER)?;
    slow.set_linger(0)?;
    slow.set_rcvhwm(1)?;
    slow.set_rcvbuf(1024)?;
    slow.set_sndtimeo(5_000)?;
    slow.set_rcvtimeo(5_000)?;
    slow.connect(&format!("tcp://{address}"))?;
    send_request(&slow, "ready", json!({"op":"get","id":1}))?;
    assert_eq!(
        receive_reply(&slow)?.get("correlation"),
        Some(&json!("ready"))
    );
    for i in 0..100 {
        send_request(&slow, &format!("load-{i}"), json!({"op":"list_page"}))?;
    }
    wait_for("reply dropped: peer queue full", Duration::from_secs(20))?;
    let other =
        json!({"identity":"other-client","agent":"b","question":"Still responsive?","options":[]});
    let before = std::time::Instant::now();
    let other_id = call(&address, json!({"op":"submit","request":other}))?
        .get("id")
        .and_then(Value::as_i64)
        .ok_or("missing other id")?;
    assert!(before.elapsed() < Duration::from_secs(10));
    assert_eq!(
        call(&address, json!({"op":"get","id":other_id}))?.get("id"),
        Some(&json!(other_id))
    );
    let request =
        json!({"identity":"slow-client-submit","agent":"a","question":"Recover?","options":[]});
    send_request(&slow, "unknown", json!({"op":"submit","request":request}))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let mut database_connection =
        runtime.block_on(SqliteConnection::connect(&format!("sqlite://{database}")))?;
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let id = loop {
        let observed: Option<Option<i64>> = runtime.block_on(
            sqlx::query_scalar("SELECT record_id FROM submission_keys WHERE identity = ?")
                .bind("slow-client-submit")
                .fetch_optional(&mut database_connection),
        )?;
        if let Some(Some(id)) = observed {
            break id;
        }
        if std::time::Instant::now() >= deadline {
            return Err("slow peer submission did not commit".into());
        }
        thread::sleep(Duration::from_millis(20));
    };
    drop(database_connection);
    assert_eq!(
        call(&address, json!({"op":"submit","request":request}))?.get("id"),
        Some(&json!(id))
    );
    drop(slow);
    drop(server);
    reader.join().map_err(|_| "log reader panicked")?;
    let server = start_server(database, &address)?;
    assert_eq!(
        call(&address, json!({"op":"submit","request":request}))?.get("id"),
        Some(&json!(id))
    );
    drop(server);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
