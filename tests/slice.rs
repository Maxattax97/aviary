use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    thread,
    time::Duration,
};

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn start(db: &str, addr: &str) -> Server {
    let child = Command::new(env!("CARGO_BIN_EXE_aviary"))
        .args(["serve", db, addr])
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let server = Server(child);
    for _ in 0..100 {
        if TcpStream::connect(addr).is_ok() {
            return server;
        }
        thread::sleep(Duration::from_millis(20));
    }
    panic!("server failed to start");
}
fn call(addr: &str, v: Value) -> Value {
    let mut s = TcpStream::connect(addr).unwrap();
    writeln!(s, "{v}").unwrap();
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
}
#[test]
fn durable_retry_and_single_answer() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    drop(listener);
    let dir = std::env::temp_dir().join(format!("aviary-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test.sqlite");
    let db = path.to_str().unwrap();
    let req = json!({"identity":"retry-1","agent":"agent","question":"Proceed?","options":["No","Yes"],"recommendation":1,"reason":"Ready"});
    let s = start(db, &addr);
    let first = call(&addr, json!({"op":"submit","request":req}));
    let id = first["id"].as_i64().unwrap();
    assert_eq!(call(&addr, json!({"op":"submit","request":req}))["id"], id);
    let mut changed = req.clone();
    changed["question"] = json!("Changed");
    assert_eq!(
        call(&addr, json!({"op":"submit","request":changed}))["error"],
        "identity conflict"
    );
    drop(s);
    let s = start(db, &addr);
    assert_eq!(
        call(&addr, json!({"op":"list"}))["requests"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        call(&addr, json!({"op":"answer","id":id,"index":1}))["id"],
        id
    );
    assert_eq!(
        call(&addr, json!({"op":"answer","id":id,"index":0}))["error"],
        "request already answered"
    );
    drop(s);
    let s = start(db, &addr);
    assert_eq!(
        call(&addr, json!({"op":"get","id":id}))["answer"],
        json!({"type":"suggested","index":1,"value":"Yes"})
    );
    assert_eq!(call(&addr, json!({"op":"submit","request":req}))["id"], id);
    drop(s);
    std::fs::remove_dir_all(dir).unwrap();
}
