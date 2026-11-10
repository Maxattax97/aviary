use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{debug, info, warn};

use crate::{
    error::{AppError, Result},
    protocol::{Operation, MAX_MESSAGE_BYTES},
    storage::Storage,
};

const TIMEOUT_MS: i32 = 10_000;
const QUEUE_LIMIT: i32 = 64;
const VERSION: u32 = 1;

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

fn configure(socket: &zmq::Socket) -> Result<()> {
    socket.set_linger(0)?;
    socket.set_sndhwm(QUEUE_LIMIT)?;
    socket.set_rcvhwm(QUEUE_LIMIT)?;
    socket.set_maxmsgsize(MAX_MESSAGE_BYTES as i64)?;
    socket.set_sndtimeo(TIMEOUT_MS)?;
    socket.set_rcvtimeo(TIMEOUT_MS)?;
    Ok(())
}

fn network_error(error: zmq::Error) -> AppError {
    if error == zmq::Error::EAGAIN {
        AppError::Timeout
    } else {
        error.into()
    }
}

fn endpoint(address: &str) -> String {
    format!("tcp://{address}")
}

pub async fn call(address: &str, operation: &Operation) -> Result<Value> {
    call_with_timeout(address, operation, TIMEOUT_MS).await
}

pub async fn call_with_timeout(
    address: &str,
    operation: &Operation,
    timeout_ms: i32,
) -> Result<Value> {
    let address = address.to_owned();
    let request = serde_json::to_vec(&RequestEnvelope {
        version: VERSION,
        // Each call has its own DEALER socket and only one outstanding request.
        correlation: "1".into(),
        operation: serde_json::from_value(serde_json::to_value(operation)?)?,
    })?;
    if request.len() > MAX_MESSAGE_BYTES as usize {
        return Err(AppError::InvalidFrame);
    }
    tokio::task::spawn_blocking(move || {
        let context = zmq::Context::new();
        let socket = context.socket(zmq::DEALER)?;
        configure(&socket)?;
        socket.set_sndtimeo(timeout_ms)?;
        socket.set_rcvtimeo(timeout_ms)?;
        socket.set_immediate(true)?;
        socket.connect(&endpoint(&address))?;
        socket.send(request, 0).map_err(network_error)?;
        let frames = socket.recv_multipart(0).map_err(network_error)?;
        let [frame] = frames.as_slice() else {
            return Err(AppError::InvalidFrame);
        };
        if frame.len() > MAX_MESSAGE_BYTES as usize {
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
    })
    .await
    .map_err(|error| AppError::Task(error.to_string()))?
}

pub async fn serve(database_path: &str, address: &str) -> Result<()> {
    let storage = Storage::open(database_path).await?;
    let address = address.to_owned();
    let runtime = tokio::runtime::Handle::current();
    // Register both handlers before the worker starts so an early stop cannot
    // strand a blocking worker while the runtime shuts down.
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let stopping = Arc::new(AtomicBool::new(false));
    let worker_stopping = Arc::clone(&stopping);
    let mut worker = tokio::task::spawn_blocking(move || {
        let context = zmq::Context::new();
        let socket = context.socket(zmq::ROUTER)?;
        configure(&socket)?;
        socket.set_rcvtimeo(100)?;
        socket.bind(&endpoint(&address))?;
        info!(%address, "service listening");
        while !worker_stopping.load(Ordering::Acquire) {
            let frames = match socket.recv_multipart(0) {
                Ok(frames) => frames,
                Err(zmq::Error::EAGAIN | zmq::Error::EINTR) => continue,
                Err(error) => return Err(error.into()),
            };
            if worker_stopping.load(Ordering::Acquire) {
                break;
            }
            let [identity, frame] = frames.as_slice() else {
                debug!("discarding invalid ZeroMQ envelope");
                continue;
            };
            if frame.len() > MAX_MESSAGE_BYTES as usize {
                continue;
            }
            let request: RequestEnvelope = match serde_json::from_slice(frame) {
                Ok(request) => request,
                Err(_) => continue,
            };
            let result = if request.version != VERSION
                || request.correlation.is_empty()
                || request.correlation.len() > 128
            {
                Err(AppError::InvalidFrame)
            } else {
                debug!("processing request");
                runtime.block_on(storage.handle(request.operation))
            };
            let result = match result {
                Ok(value) => value,
                Err(error) => {
                    if matches!(error, AppError::Database(_) | AppError::Io(_)) {
                        warn!(%error, "request failed");
                    }
                    json!({"error": error.to_string()})
                }
            };
            let mut response = serde_json::to_vec(&ResponseEnvelope {
                version: VERSION,
                correlation: request.correlation.clone(),
                result,
            })?;
            if response.len() > MAX_MESSAGE_BYTES as usize {
                response = serde_json::to_vec(&ResponseEnvelope {
                    version: VERSION,
                    correlation: request.correlation,
                    result: json!({"error": "response too large"}),
                })?;
            }
            if let Err(error) = socket.send_multipart([identity.as_slice(), response.as_slice()], 0)
            {
                debug!(%error, "reply could not be sent");
            }
        }
        info!("service stopped");
        Ok(())
    });
    tokio::select! {
        result = &mut worker => result.map_err(|error| AppError::Task(error.to_string()))?,
        _ = interrupt.recv() => {
            info!("shutdown requested");
            stopping.store(true, Ordering::Release);
            worker.await.map_err(|error| AppError::Task(error.to_string()))?
        },
        _ = terminate.recv() => {
            info!("shutdown requested");
            stopping.store(true, Ordering::Release);
            worker.await.map_err(|error| AppError::Task(error.to_string()))?
        }
    }
}
