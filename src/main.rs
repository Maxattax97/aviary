use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    env,
    io::{self, BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::mpsc,
    thread,
    time::Duration,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Request {
    identity: String,
    agent: String,
    question: String,
    options: Vec<String>,
    recommendation: usize,
    reason: String,
}

fn db(path: &str) -> Result<Connection, String> {
    let c = Connection::open(path).map_err(|e| e.to_string())?;
    c.busy_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS requests (id INTEGER PRIMARY KEY, identity TEXT NOT NULL UNIQUE, payload TEXT NOT NULL, answer TEXT);").map_err(|e| e.to_string())?;
    Ok(c)
}
fn handle(c: &mut Connection, v: Value) -> Result<Value, String> {
    let op = v.get("op").and_then(Value::as_str).ok_or("missing op")?;
    match op {
        "submit" => {
            let req: Request =
                serde_json::from_value(v.get("request").cloned().ok_or("missing request")?)
                    .map_err(|e| e.to_string())?;
            if req.identity.is_empty()
                || req.agent.is_empty()
                || req.question.is_empty()
                || req.reason.is_empty()
                || req.options.is_empty()
                || req.options.iter().any(|s| s.is_empty())
                || req.recommendation >= req.options.len()
            {
                return Err("invalid request".into());
            }
            let payload = serde_json::to_string(&req).map_err(|e| e.to_string())?;
            let tx = c.transaction().map_err(|e| e.to_string())?;
            let existing: Option<(i64, String)> = tx
                .query_row(
                    "SELECT id,payload FROM requests WHERE identity=?1",
                    [&req.identity],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            let id = if let Some((id, old)) = existing {
                if old != payload {
                    return Err("identity conflict".into());
                }
                id
            } else {
                tx.execute(
                    "INSERT INTO requests(identity,payload) VALUES (?1,?2)",
                    params![req.identity, payload],
                )
                .map_err(|e| e.to_string())?;
                tx.last_insert_rowid()
            };
            tx.commit().map_err(|e| e.to_string())?;
            Ok(json!({"id":id}))
        }
        "list" => {
            let mut s = c
                .prepare("SELECT id,payload FROM requests WHERE answer IS NULL ORDER BY id")
                .map_err(|e| e.to_string())?;
            let rows = s
                .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
                .map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            for row in rows {
                let (id, payload) = row.map_err(|e| e.to_string())?;
                let request: Request = serde_json::from_str(&payload).map_err(|e| e.to_string())?;
                out.push(json!({"id":id,"request":request}));
            }
            Ok(json!({"requests":out}))
        }
        "get" => {
            let id = v.get("id").and_then(Value::as_i64).ok_or("missing id")?;
            let answer: Option<Option<String>> = c
                .query_row("SELECT answer FROM requests WHERE id=?1", [id], |r| {
                    r.get(0)
                })
                .optional()
                .map_err(|e| e.to_string())?;
            match answer {
                Some(Some(a)) => Ok(
                    json!({"id":id,"answer":serde_json::from_str::<Value>(&a).map_err(|e| e.to_string())?}),
                ),
                Some(None) => Ok(json!({"id":id,"answer":null})),
                None => Err("request not found".into()),
            }
        }
        "answer" => {
            let id = v.get("id").and_then(Value::as_i64).ok_or("missing id")?;
            let index = v
                .get("index")
                .and_then(Value::as_u64)
                .ok_or("missing index")? as usize;
            let tx = c.transaction().map_err(|e| e.to_string())?;
            let row: Option<(String, Option<String>)> = tx
                .query_row(
                    "SELECT payload,answer FROM requests WHERE id=?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            let (payload, existing) = row.ok_or("request not found")?;
            if existing.is_some() {
                return Err("request already answered".into());
            }
            let req: Request = serde_json::from_str(&payload).map_err(|e| e.to_string())?;
            let value = req.options.get(index).ok_or("invalid option")?;
            let answer = json!({"type":"suggested","index":index,"value":value}).to_string();
            let changed = tx
                .execute(
                    "UPDATE requests SET answer=?1 WHERE id=?2 AND answer IS NULL",
                    params![answer, id],
                )
                .map_err(|e| e.to_string())?;
            if changed != 1 {
                return Err("request already answered".into());
            }
            tx.commit().map_err(|e| e.to_string())?;
            Ok(json!({"id":id}))
        }
        _ => Err("unknown op".into()),
    }
}
fn call(addr: &str, v: Value) -> Result<Value, String> {
    let mut stream = TcpStream::connect(addr).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|e| e.to_string())?;
    writeln!(stream, "{v}").map_err(|e| e.to_string())?;
    let mut line = String::new();
    BufReader::new(stream)
        .take(1_048_577)
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    let result: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
    if let Some(e) = result.get("error").and_then(Value::as_str) {
        Err(e.into())
    } else {
        Ok(result)
    }
}
fn serve(path: &str, addr: &str) -> Result<(), String> {
    db(path)?;
    let listener = TcpListener::bind(addr).map_err(|e| e.to_string())?;
    eprintln!(
        "listening on {}",
        listener.local_addr().map_err(|e| e.to_string())?
    );
    for stream in listener.incoming() {
        let stream = stream.map_err(|e| e.to_string())?;
        let path = path.to_owned();
        thread::spawn(move || {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
            let mut line = String::new();
            let result = (|| -> Result<Value, String> {
                let mut reader = BufReader::new(&stream).take(1_048_577);
                reader.read_line(&mut line).map_err(|e| e.to_string())?;
                if line.len() > 1_048_576 || !line.ends_with('\n') {
                    return Err("message too large or incomplete".into());
                }
                let v = serde_json::from_str(&line).map_err(|e| e.to_string())?;
                handle(&mut db(&path)?, v)
            })();
            let out = result.unwrap_or_else(|e| json!({"error":e}));
            let _ = writeln!(&stream, "{out}");
        });
    }
    Ok(())
}
fn tui(addr: &str) -> Result<(), String> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut shown = String::new();
    loop {
        let data = call(addr, json!({"op":"list"}))?;
        let requests = data["requests"].as_array().ok_or("invalid list")?;
        let display = serde_json::to_string(requests).map_err(|e| e.to_string())?;
        if display != shown {
            print!("\x1b[2J\x1b[H");
            if let Some(item) = requests.first() {
                let req: Request =
                    serde_json::from_value(item["request"].clone()).map_err(|e| e.to_string())?;
                println!(
                    "Request {} from {}\n{}\n\n1. {} (recommended: {})",
                    item["id"],
                    req.agent,
                    req.question,
                    req.options[req.recommendation],
                    req.reason
                );
                for (i, opt) in req
                    .options
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != req.recommendation)
                {
                    println!("   {} (original index {})", opt, i);
                }
                println!("\nPress Enter to confirm recommendation; q then Enter to quit.");
            } else {
                println!("No pending requests. q then Enter to quit.");
            }
            io::stdout().flush().map_err(|e| e.to_string())?;
            shown = display;
        }
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(Ok(line)) if line == "q" => return Ok(()),
            Ok(Ok(line)) if line.is_empty() => {
                if let Some(item) = requests.first() {
                    let req: Request = serde_json::from_value(item["request"].clone())
                        .map_err(|e| e.to_string())?;
                    call(
                        addr,
                        json!({"op":"answer","id":item["id"],"index":req.recommendation}),
                    )?;
                }
            }
            Ok(Err(e)) => return Err(e.to_string()),
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
            _ => (),
        }
    }
}
fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("serve") => serve(args.get(2).map(String::as_str).unwrap_or("aviary.sqlite"),args.get(3).map(String::as_str).unwrap_or("127.0.0.1:7432")),
        Some("tui") => tui(args.get(2).map(String::as_str).unwrap_or("127.0.0.1:7432")),
        Some("call") => { let addr = args.get(2).ok_or("call requires address and JSON")?; let v: Value = serde_json::from_str(args.get(3).ok_or("call requires address and JSON")?).map_err(|e| e.to_string())?; println!("{}",call(addr,v)?); Ok(()) },
        Some("wait") => { let addr = args.get(2).ok_or("wait requires address and id")?; let id: i64 = args.get(3).ok_or("wait requires address and id")?.parse::<i64>().map_err(|e| e.to_string())?; loop { let v = call(addr,json!({"op":"get","id":id}))?; if !v["answer"].is_null() { println!("{v}"); return Ok(()); } thread::sleep(Duration::from_millis(500)); } },
        _ => Err("usage: aviary serve [db] [bind] | tui [address] | call <address> <json> | wait <address> <id>".into())
    }
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
