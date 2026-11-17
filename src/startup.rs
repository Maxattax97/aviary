//! Local operator entry point and service identification.

use std::{
    fs::{self, OpenOptions},
    future::Future,
    io::{BufRead, BufReader},
    net::{SocketAddr, TcpStream},
    os::unix::{fs::OpenOptionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use crate::{
    error::{AppError, Result},
    network,
    protocol::Operation,
};

const DATABASE: &str = "aviary.sqlite";
const LOG: &str = "aviary-service.log";
const STARTUP_TIMEOUT: Duration = Duration::from_secs(15);
const SERVICE_CHECK_TIMEOUT: Duration = Duration::from_secs(5);

fn remaining(deadline: Instant) -> Result<Duration> {
    let duration = deadline.saturating_duration_since(Instant::now());
    if duration.is_zero() {
        Err(AppError::Timeout)
    } else {
        Ok(duration)
    }
}

async fn within_deadline<T>(
    deadline: Instant,
    maximum: Duration,
    future: impl Future<Output = T>,
) -> Result<T> {
    tokio::time::timeout(remaining(deadline)?.min(maximum), future)
        .await
        .map_err(|_| AppError::Timeout)
}

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

async fn verify_service(address: &str, expected: &Path, deadline: Instant) -> Result<()> {
    let budget = remaining(deadline)?.min(SERVICE_CHECK_TIMEOUT);
    let timeout_ms = budget.as_millis().min(i32::MAX as u128) as i32;
    let info = within_deadline(
        deadline,
        SERVICE_CHECK_TIMEOUT,
        network::call_with_timeout(address, &Operation::ServiceInfo, timeout_ms),
    )
    .await
    .map_err(|error| AppError::Task(format!("Cannot identify service at {address}: {error}")))?
    .map_err(|error| AppError::Task(format!("Cannot identify service at {address}: {error}")))?;
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
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    let expected = expected_database()?;
    let address: SocketAddr = endpoint
        .parse()
        .map_err(|error| AppError::Task(format!("invalid default address: {error}")))?;
    if !address.is_ipv4() || !address.ip().is_loopback() || address.port() == 0 {
        return Err(AppError::Task(
            "local startup requires a nonzero IPv4 loopback address".into(),
        ));
    }
    let connect_budget = remaining(deadline)?.min(Duration::from_millis(250));
    if TcpStream::connect_timeout(&address, connect_budget).is_ok() {
        return verify_service(endpoint, &expected, deadline).await;
    }
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(LOG)?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(["serve", DATABASE, endpoint, "--ready"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(log));
    // SAFETY: setsid is async-signal-safe and the closure performs no
    // allocation or locking between fork and exec. Fail startup if detaching
    // the service from the operator's terminal session fails.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command.spawn()?;
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
    let line = within_deadline(deadline, STARTUP_TIMEOUT, readiness)
        .await
        .map_err(|_| {
            AppError::Task(format!(
                "Local startup timed out; the service may still start. Check {LOG}"
            ))
        })?
        .map_err(|error| AppError::Task(format!("service readiness task failed: {error}")))??;
    if line.trim() != "READY" {
        // Another concurrent launcher may have won the bind. It can serve
        // requests only after opening its database; verify its identity.
        return verify_service(endpoint, &expected, deadline)
            .await
            .map_err(|error| {
                AppError::Task(format!(
                    "Service failed to start or is incompatible: {error}; check {LOG}"
                ))
            });
    }
    verify_service(endpoint, &expected, deadline).await
}

#[cfg(test)]
mod tests {
    use super::within_deadline;
    use std::time::{Duration, Instant};

    #[tokio::test]
    async fn startup_phases_share_one_deadline() {
        let deadline = Instant::now() + Duration::from_millis(100);
        within_deadline(
            deadline,
            Duration::from_secs(1),
            tokio::time::sleep(Duration::from_millis(30)),
        )
        .await
        .expect("first phase fits");
        let error = within_deadline(
            deadline,
            Duration::from_secs(1),
            tokio::time::sleep(Duration::from_secs(1)),
        )
        .await
        .expect_err("second phase uses the remaining budget");
        assert!(error.to_string().contains("timed out"));
    }
}
