//! Local operator entry point and service identification.

use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader},
    net::{SocketAddr, TcpStream},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

use crate::{
    error::{AppError, Result},
    network,
    protocol::Operation,
};

const DATABASE: &str = "aviary.sqlite";
const LOG: &str = "aviary-service.log";

fn expected_database() -> Result<PathBuf> {
    let path = std::env::current_dir()?.join(DATABASE);
    match fs::canonicalize(&path) {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // A dangling symlink must not be mistaken for a new database.
            match fs::symlink_metadata(&path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    let parent = path
                        .parent()
                        .ok_or_else(|| AppError::Task("database has no parent directory".into()))?;
                    Ok(fs::canonicalize(parent)?.join(DATABASE))
                }
                Err(error) => Err(error.into()),
                Ok(_) => Err(error.into()),
            }
        }
        Err(error) => Err(error.into()),
    }
}

async fn verify_service(address: &str, expected: &Path) -> Result<()> {
    let info = network::call_with_timeout(address, &Operation::ServiceInfo, 5_000)
        .await
        .map_err(|error| {
            AppError::Task(format!("Cannot identify service at {address}: {error}"))
        })?;
    if info.get("service").and_then(serde_json::Value::as_str) != Some("aviary")
        || info.get("protocol").and_then(serde_json::Value::as_u64) != Some(2)
        || info
            .get("database")
            .and_then(serde_json::Value::as_str)
            .map(Path::new)
            != Some(expected)
    {
        return Err(AppError::Task(format!(
            "Service at {address} is incompatible or uses a different database; expected {}",
            expected.display()
        )));
    }
    Ok(())
}

pub async fn ensure_local(endpoint: &str) -> Result<()> {
    let expected = expected_database()?;
    let address: SocketAddr = endpoint
        .parse()
        .map_err(|error| AppError::Task(format!("invalid default address: {error}")))?;
    if !address.is_ipv4() || !address.ip().is_loopback() || address.port() == 0 {
        return Err(AppError::Task(
            "local startup requires a nonzero IPv4 loopback address".into(),
        ));
    }
    if TcpStream::connect_timeout(&address, Duration::from_millis(250)).is_ok() {
        return verify_service(endpoint, &expected).await;
    }
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(LOG)?;
    let mut child = Command::new(std::env::current_exe()?)
        .args(["serve", DATABASE, endpoint, "--ready"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(log))
        .spawn()?;
    let output = child
        .stdout
        .take()
        .ok_or_else(|| AppError::Task("service readiness pipe unavailable".into()))?;
    // Reap a failed child or a service that later exits while this TUI remains
    // open. This detached thread does not keep the process alive on exit.
    std::thread::Builder::new()
        .name("aviary-service-reaper".into())
        .spawn(move || {
            let _ = child.wait();
        })?;
    let (sender, readiness) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name("aviary-service-ready".into())
        .spawn(move || {
            let mut line = String::new();
            let result = BufReader::new(output).read_line(&mut line).map(|_| line);
            let _ = sender.send(result);
        })?;
    let line = tokio::time::timeout(Duration::from_secs(15), readiness)
        .await
        .map_err(|_| {
            AppError::Task(format!(
                "Service startup timed out; it may still start. Check {LOG}"
            ))
        })?
        .map_err(|error| AppError::Task(format!("service readiness task failed: {error}")))??;
    if line.trim() != "READY" {
        // Another concurrent launcher may have won the bind. It can serve
        // requests only after opening its database; verify its identity.
        return verify_service(endpoint, &expected).await.map_err(|error| {
            AppError::Task(format!(
                "Service failed to start or is incompatible: {error}; check {LOG}"
            ))
        });
    }
    verify_service(endpoint, &expected).await
}
