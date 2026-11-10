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
    let context = zmq::Context::new();
    let socket = context.socket(zmq::DEALER)?;
    socket.set_linger(0)?;
    socket.set_rcvtimeo(10_000)?;
    socket.connect(&format!("tcp://{address}"))?;
    socket.send(
        json!({"version":1,"correlation":"test","operation":request})
            .to_string()
            .as_bytes(),
        0,
    )?;
    let response = socket.recv_bytes(0)?;
    let envelope: Value = serde_json::from_slice(&response)?;
    assert_eq!(envelope.get("version"), Some(&json!(1)));
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
                call(
                    &address,
                    json!({"op":"answer", "id":concurrent_id, "index":index % 2}),
                )
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
    assert!(!call(&address, json!({"op":"get", "id":concurrent_id}))?["answer"].is_null());
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
