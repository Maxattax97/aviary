use std::time::Duration;

use serde_json::{json, Value};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    time::timeout,
};
use tracing::{debug, info, warn};

use crate::{
    error::{AppError, Result},
    protocol::{Operation, MAX_MESSAGE_BYTES},
    storage::Storage,
};

const NETWORK_TIMEOUT: Duration = Duration::from_secs(10);

async fn read_frame(reader: impl tokio::io::AsyncRead + Unpin) -> Result<String> {
    let mut frame = String::new();
    BufReader::new(reader)
        .take(MAX_MESSAGE_BYTES + 1)
        .read_line(&mut frame)
        .await?;
    if frame.len() as u64 > MAX_MESSAGE_BYTES || !frame.ends_with('\n') {
        return Err(AppError::InvalidFrame);
    }
    Ok(frame)
}

pub async fn call(address: &str, operation: &Operation) -> Result<Value> {
    let mut stream = TcpStream::connect(address).await?;
    let request = serde_json::to_string(operation)?;
    stream.write_all(request.as_bytes()).await?;
    stream.write_all(b"\n").await?;
    let frame = timeout(NETWORK_TIMEOUT, read_frame(stream))
        .await
        .map_err(|_| AppError::Timeout)??;
    let response: Value = serde_json::from_str(&frame)?;
    if let Some(error) = response.get("error").and_then(Value::as_str) {
        return Err(AppError::Remote(error.to_owned()));
    }
    Ok(response)
}

pub async fn serve(database_path: &str, address: &str) -> Result<()> {
    let storage = Storage::open(database_path).await?;
    let listener = TcpListener::bind(address).await?;
    info!(address = %listener.local_addr()?, "service listening");

    loop {
        let (stream, peer) = listener.accept().await?;
        let storage = storage.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_connection(stream, storage).await {
                debug!(%peer, %error, "connection ended without a response");
            }
        });
    }
}

async fn handle_connection(mut stream: TcpStream, storage: Storage) -> Result<()> {
    let result = timeout(NETWORK_TIMEOUT, read_frame(&mut stream)).await;
    let response = match result {
        Ok(Ok(frame)) => {
            let operation = serde_json::from_str::<Operation>(&frame);
            match operation {
                Ok(operation) => storage.handle(operation).await,
                Err(error) => Err(error.into()),
            }
        }
        Ok(Err(error)) => Err(error),
        Err(_) => Err(AppError::Timeout),
    };
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            // Log operational failures without logging request content.
            if matches!(error, AppError::Database(_) | AppError::Io(_)) {
                warn!(%error, "request failed");
            }
            json!({"error": error.to_string()})
        }
    };
    let encoded = serde_json::to_string(&response)?;
    stream.write_all(encoded.as_bytes()).await?;
    stream.write_all(b"\n").await?;
    Ok(())
}
