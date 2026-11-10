use serde_json::{json, Value};
use std::{
    error::Error,
    io::{BufRead, BufReader, Write},
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
    let mut stream = TcpStream::connect(address)?;
    writeln!(stream, "{request}")?;
    let mut response = String::new();
    BufReader::new(stream).read_line(&mut response)?;
    Ok(serde_json::from_str(&response)?)
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
