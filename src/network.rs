use crate::{
    error::{AppError, Result},
    protocol::{Operation, MAX_MESSAGE_BYTES, MAX_RESPONSE_BYTES},
    storage::Storage,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    fs::{File, OpenOptions},
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    time::Duration,
};
use tokio::sync::watch;
use zeromq::{
    DealerSocket, RouterSocket, Socket, SocketEvent, SocketOptions, SocketRecv, SocketSend,
    ZmqError, ZmqMessage,
};

const VERSION: u32 = 2;
const TIMEOUT_MS: i32 = 10_000;
const MAX_SUBSCRIBERS: usize = 256;

fn lock_database(path: &str) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)?;
    // SAFETY: flock operates on this live file descriptor and does not retain
    // the pointer or access Rust memory. Keep the file open for service life.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        let error = std::io::Error::last_os_error();
        return if error.kind() == std::io::ErrorKind::WouldBlock {
            Err(AppError::Task(format!(
                "database is already in use by another service: {path}"
            )))
        } else {
            Err(AppError::Task(format!(
                "cannot lock database {path}: {error}"
            )))
        };
    }
    Ok(file)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestEnvelope {
    version: u32,
    correlation: String,
    operation: Operation,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResponseEnvelope {
    version: u32,
    correlation: String,
    result: Value,
}

pub async fn call(address: &str, operation: &Operation) -> Result<Value> {
    call_with_timeout(address, operation, TIMEOUT_MS).await
}
pub async fn call_with_timeout(
    address: &str,
    operation: &Operation,
    timeout_ms: i32,
) -> Result<Value> {
    let request = serde_json::to_vec(&RequestEnvelope {
        version: VERSION,
        correlation: "1".into(),
        operation: serde_json::from_value(serde_json::to_value(operation)?)?,
    })?;
    if request.len() > MAX_MESSAGE_BYTES as usize {
        return Err(AppError::InvalidFrame);
    }
    let duration = Duration::from_millis(timeout_ms.max(0) as u64);
    let mut options = SocketOptions::default();
    options.message_limits(
        MAX_RESPONSE_BYTES as usize,
        MAX_RESPONSE_BYTES as usize + 4096,
        2,
    );
    let mut socket = DealerSocket::with_options(options);
    tokio::time::timeout(duration, socket.connect(&format!("tcp://{address}")))
        .await
        .map_err(|_| AppError::Timeout)??;
    tokio::time::timeout(duration, socket.send(ZmqMessage::from(request)))
        .await
        .map_err(|_| AppError::Timeout)??;
    let response = tokio::time::timeout(duration, socket.recv())
        .await
        .map_err(|_| AppError::Timeout)??;
    let frames = response.into_vec();
    if frames.len() != 1 {
        return Err(AppError::InvalidFrame);
    }
    let frame = frames.first().ok_or(AppError::InvalidFrame)?;
    if frame.len() > MAX_RESPONSE_BYTES as usize {
        return Err(AppError::InvalidFrame);
    }
    let response: ResponseEnvelope = serde_json::from_slice(frame)?;
    if response.version != VERSION || response.correlation != "1" {
        return Err(AppError::InvalidFrame);
    }
    if let Some(error) = response.result.get("error").and_then(Value::as_str) {
        return Err(AppError::Remote(error.to_owned()));
    }
    Ok(response.result)
}

/// A change subscription is a hint; reconnect requires a durable-state read.
#[derive(Clone, Copy, Default)]
pub struct ChangeState {
    pub connected: bool,
    pub revision: u64,
}
pub struct ChangeWatch(tokio::task::JoinHandle<()>);
impl Drop for ChangeWatch {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub fn watch_changes(address: &str) -> (ChangeWatch, watch::Receiver<ChangeState>) {
    let address = address.to_owned();
    let (sender, receiver) = watch::channel(ChangeState::default());
    let task = tokio::spawn(async move {
        let mut reported_disconnected = false;
        loop {
            if let Err(error) = watch_connection(&address, &sender).await {
                tracing::debug!(%error, "change subscription disconnected");
            }
            if sender.borrow().connected || !reported_disconnected {
                sender.send_modify(|state| state.connected = false);
                reported_disconnected = true;
            }
            // Reconnect after a failed connection; no state reads are polled.
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
    (ChangeWatch(task), receiver)
}

async fn watch_connection(address: &str, changes: &watch::Sender<ChangeState>) -> Result<()> {
    let mut options = SocketOptions::default();
    options.message_limits(
        MAX_RESPONSE_BYTES as usize,
        MAX_RESPONSE_BYTES as usize + 4096,
        2,
    );
    let mut socket = DealerSocket::with_options(options);
    let mut monitor = socket.monitor();
    let timeout = Duration::from_millis(TIMEOUT_MS as u64);
    tokio::time::timeout(timeout, socket.connect(&format!("tcp://{address}")))
        .await
        .map_err(|_| AppError::Timeout)??;
    let request = serde_json::to_vec(&RequestEnvelope {
        version: VERSION,
        correlation: "subscribe".into(),
        operation: Operation::SubscribeChanges,
    })?;
    tokio::time::timeout(timeout, socket.send(ZmqMessage::from(request)))
        .await
        .map_err(|_| AppError::Timeout)??;
    let response = tokio::time::timeout(timeout, socket.recv())
        .await
        .map_err(|_| AppError::Timeout)??;
    let frames = response.into_vec();
    if frames.len() != 1 {
        return Err(AppError::InvalidFrame);
    }
    let response: ResponseEnvelope =
        serde_json::from_slice(frames.first().ok_or(AppError::InvalidFrame)?)?;
    if response.version != VERSION
        || response.correlation != "subscribe"
        || response.result.get("watching").and_then(Value::as_bool) != Some(true)
    {
        return Err(AppError::InvalidFrame);
    }
    changes.send_modify(|state| {
        state.connected = true;
        state.revision = state.revision.wrapping_add(1);
    });
    loop {
        tokio::select! {
            message = socket.recv() => {
                let frames = message?.into_vec();
                if frames.len() != 1 { return Err(AppError::InvalidFrame); }
                let response: ResponseEnvelope = serde_json::from_slice(frames.first().ok_or(AppError::InvalidFrame)?)?;
                if response.version != VERSION || response.correlation != "event" || response.result.get("changed").and_then(Value::as_bool) != Some(true) { return Err(AppError::InvalidFrame); }
                changes.send_modify(|state| state.revision = state.revision.wrapping_add(1));
            }
            event = monitor.next() => {
                if matches!(event, None | Some(SocketEvent::Disconnected(_))) { return Err(AppError::Task("change subscription disconnected".into())); }
            }
        }
    }
}

pub async fn serve(database_path: &str, address: &str, ready: bool) -> Result<()> {
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut options = SocketOptions::default();
    options.message_limits(
        MAX_MESSAGE_BYTES as usize,
        MAX_MESSAGE_BYTES as usize + 4096,
        2,
    );
    let mut socket = RouterSocket::with_options(options);
    let mut monitor = socket.monitor();
    let mut subscribers: HashSet<bytes::Bytes> = HashSet::new();
    let mut used_peers: HashSet<bytes::Bytes> = HashSet::new();
    socket.bind(&format!("tcp://{address}")).await?;
    // Acquire the listener before opening the database: a competing service
    // must not initialize a different database after losing the bind race.
    let _database_lock = lock_database(database_path)?;
    let storage = Storage::open(database_path).await?;
    let database = std::fs::canonicalize(database_path)?
        .to_string_lossy()
        .into_owned();
    tracing::info!(%address, "service listening");
    if ready {
        use std::io::Write as _;
        println!("READY");
        std::io::stdout().flush()?;
    }
    let mut stopping = false;
    loop {
        let message = tokio::select! {
            _ = interrupt.recv() => break,
            _ = terminate.recv() => break,
            event = monitor.next() => {
                if let Some(SocketEvent::Disconnected(peer)) = event {
                    subscribers.retain(|identity| identity.as_ref() != peer.as_ref());
                    used_peers.retain(|identity| identity.as_ref() != peer.as_ref());
                }
                continue;
            }
            result = socket.recv() => result?,
        };
        let frames = message.into_vec();
        if frames.len() != 2 {
            continue;
        }
        let Some(identity) = frames.first() else {
            continue;
        };
        // Subscribers are receive-only after their initial acknowledgment.
        // This keeps ordinary replies out of their bounded outbound queue.
        if subscribers.contains(identity) {
            continue;
        }
        if used_peers.len() >= 1024 {
            used_peers.retain(|peer| socket.has_peer(peer.as_ref()));
        }
        // A subscription is accepted only as the first message on its peer;
        // otherwise old replies could occupy the queue when hints coalesce.
        let fresh = used_peers.insert(identity.clone());
        let Some(frame) = frames.get(1) else {
            continue;
        };
        if frame.len() > MAX_MESSAGE_BYTES as usize {
            continue;
        }
        let Ok(request) = serde_json::from_slice::<RequestEnvelope>(frame) else {
            continue;
        };
        let subscribing = matches!(&request.operation, Operation::SubscribeChanges);
        let mutation = matches!(
            &request.operation,
            Operation::Submit { .. }
                | Operation::SetImportance { .. }
                | Operation::SetDeferred { .. }
                | Operation::DeferDecision { .. }
                | Operation::SwapDecisions { .. }
                | Operation::Answer { .. }
                | Operation::AnswerCustom { .. }
                | Operation::CreateChannel { .. }
                | Operation::SendMessage { .. }
                | Operation::ReplyMessage { .. }
                | Operation::SubmitNotice { .. }
                | Operation::DismissNotice { .. }
        );
        tracing::debug!("processing request");
        let result = if !matches!(request.version, 1 | VERSION)
            || request.correlation.is_empty()
            || request.correlation.len() > 128
        {
            Err(AppError::InvalidFrame)
        } else if matches!(&request.operation, Operation::ServiceInfo) {
            Ok(
                json!({"service":"aviary", "protocol":VERSION, "database":database, "pid":std::process::id()}),
            )
        } else if subscribing {
            if subscribers.len() >= MAX_SUBSCRIBERS {
                subscribers.retain(|identity| socket.has_peer(identity.as_ref()));
            }
            if request.version != VERSION
                || !fresh
                || (subscribers.len() >= MAX_SUBSCRIBERS && !subscribers.contains(identity))
            {
                Err(AppError::InvalidRequest)
            } else {
                Ok(json!({"watching": true}))
            }
        } else {
            let work = storage.handle(request.operation);
            tokio::pin!(work);
            tokio::select! {
                result = &mut work => result,
                _ = interrupt.recv() => { tracing::info!("shutdown requested"); stopping = true; work.await },
                _ = terminate.recv() => { tracing::info!("shutdown requested"); stopping = true; work.await }
            }
        };
        let accepted = result.is_ok();
        let result = match result {
            Ok(value) => value,
            Err(error) => {
                if matches!(error, AppError::Database(_) | AppError::Io(_)) {
                    tracing::warn!(%error, "request failed");
                }
                json!({"error": error.to_string()})
            }
        };
        let version = if request.version == VERSION {
            VERSION
        } else {
            1
        };
        let mut response = serde_json::to_vec(&ResponseEnvelope {
            version,
            correlation: request.correlation.clone(),
            result,
        })?;
        let max = if version == 1 {
            MAX_MESSAGE_BYTES
        } else {
            MAX_RESPONSE_BYTES
        };
        if response.len() > max as usize {
            response = serde_json::to_vec(&ResponseEnvelope {
                version,
                correlation: request.correlation,
                result: json!({"error":"response too large"}),
            })?;
        }
        let mut reply = ZmqMessage::from(identity.clone());
        reply.push_back(bytes::Bytes::from(response));
        let sent = socket.try_send(reply).await;
        if sent.is_err() {
            tracing::debug!("reply dropped: peer queue full");
        }
        if subscribing && accepted && sent.is_ok() {
            subscribers.insert(identity.clone());
        }
        if mutation && accepted {
            let payload = serde_json::to_vec(&ResponseEnvelope {
                version: VERSION,
                correlation: "event".into(),
                result: json!({"changed": true}),
            })?;
            let mut disconnected = Vec::new();
            for subscriber in &subscribers {
                let mut notification = ZmqMessage::from(subscriber.clone());
                notification.push_back(bytes::Bytes::copy_from_slice(&payload));
                match socket.try_send(notification).await {
                    Ok(()) | Err(ZmqError::BufferFull(_)) => {
                        // A full queue already contains change hints, which
                        // cause a durable-state read after the subscriber catches up.
                    }
                    Err(_) => disconnected.push(subscriber.clone()),
                }
            }
            for subscriber in disconnected {
                subscribers.remove(&subscriber);
            }
        }
        if stopping {
            break;
        }
    }
    Ok(())
}
