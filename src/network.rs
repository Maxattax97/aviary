use crate::{
    error::{AppError, Result},
    protocol::{Operation, MAX_MESSAGE_BYTES, MAX_RESPONSE_BYTES},
    storage::Storage,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use zeromq::{
    DealerSocket, RouterSocket, Socket, SocketOptions, SocketRecv, SocketSend, ZmqMessage,
};

const VERSION: u32 = 2;
const TIMEOUT_MS: i32 = 10_000;
// Bound reply queue waits so one slow peer cannot stall other clients.
const REPLY_ENQUEUE_TIMEOUT: Duration = Duration::from_millis(1);
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

pub async fn serve(database_path: &str, address: &str) -> Result<()> {
    let storage = Storage::open(database_path).await?;
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut options = SocketOptions::default();
    options.message_limits(
        MAX_MESSAGE_BYTES as usize,
        MAX_MESSAGE_BYTES as usize + 4096,
        2,
    );
    let mut socket = RouterSocket::with_options(options);
    socket.bind(&format!("tcp://{address}")).await?;
    tracing::info!(%address, "service listening");
    let mut stopping = false;
    loop {
        let message = tokio::select! { _ = interrupt.recv() => break, _ = terminate.recv() => break, result = socket.recv() => result? };
        let frames = message.into_vec();
        if frames.len() != 2 {
            continue;
        }
        let Some(identity) = frames.first() else {
            continue;
        };
        let Some(frame) = frames.get(1) else {
            continue;
        };
        if frame.len() > MAX_MESSAGE_BYTES as usize {
            continue;
        }
        let Ok(request) = serde_json::from_slice::<RequestEnvelope>(frame) else {
            continue;
        };
        tracing::debug!("processing request");
        let result = if !matches!(request.version, 1 | VERSION)
            || request.correlation.is_empty()
            || request.correlation.len() > 128
        {
            Err(AppError::InvalidFrame)
        } else {
            let work = storage.handle(request.operation);
            tokio::pin!(work);
            tokio::select! {
                result = &mut work => result,
                _ = interrupt.recv() => { tracing::info!("shutdown requested"); stopping = true; work.await },
                _ = terminate.recv() => { tracing::info!("shutdown requested"); stopping = true; work.await }
            }
        };
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
        if tokio::time::timeout(REPLY_ENQUEUE_TIMEOUT, socket.send(reply))
            .await
            .is_err()
        {
            tracing::debug!("reply dropped: peer queue full");
        }
        if stopping {
            break;
        }
    }
    Ok(())
}
