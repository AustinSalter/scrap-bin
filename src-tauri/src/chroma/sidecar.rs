use parking_lot::Mutex;
use serde::Serialize;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use super::client::get_client_with_port;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(thiserror::Error, Debug)]
pub enum SidecarError {
    #[error("Chroma binary not found on PATH")]
    BinaryNotFound,
    #[error("Failed to spawn process: {0}")]
    SpawnFailed(String),
    #[error("Health check timed out after {0}s")]
    HealthTimeout(u64),
    #[error("Max restarts ({0}) exceeded")]
    MaxRestartsExceeded(u32),
    #[error("Not running")]
    NotRunning,
    #[error("Already running")]
    AlreadyRunning,
    #[error("Stop failed: {0}")]
    StopFailed(String),
}

impl Serialize for SidecarError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const HEALTH_DEADLINE: Duration = Duration::from_secs(10);
const HEALTH_POLL_INTERVAL: Duration = Duration::from_millis(500);
const STOP_GRACE_PERIOD: Duration = Duration::from_secs(5);
const MAX_RESTARTS: u32 = 3;

// ---------------------------------------------------------------------------
// ChromaSidecar
// ---------------------------------------------------------------------------

struct ChromaSidecar {
    process: Option<Child>,
    persist_dir: PathBuf,
    port: u16,
    started_at: Instant,
    restart_count: u32,
}

static SIDECAR: Mutex<Option<ChromaSidecar>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// Binary resolution
// ---------------------------------------------------------------------------

fn find_chroma_binary() -> Result<PathBuf, SidecarError> {
    let output = Command::new("which")
        .arg("chroma")
        .output()
        .map_err(|_| SidecarError::BinaryNotFound)?;

    if !output.status.success() {
        return Err(SidecarError::BinaryNotFound);
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        return Err(SidecarError::BinaryNotFound);
    }

    Ok(PathBuf::from(path))
}

// ---------------------------------------------------------------------------
// Health polling
// ---------------------------------------------------------------------------

fn poll_health(port: u16) -> bool {
    let client = get_client_with_port(port);
    let deadline = Instant::now() + HEALTH_DEADLINE;

    while Instant::now() < deadline {
        // Use a small tokio runtime for synchronous context.
        let result = tokio::runtime::Handle::try_current()
            .map(|handle| handle.block_on(client.heartbeat()))
            .unwrap_or_else(|_| {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(client.heartbeat())
            });

        if result.is_ok() {
            return true;
        }

        std::thread::sleep(HEALTH_POLL_INTERVAL);
    }

    false
}

// ---------------------------------------------------------------------------
// Process management helpers
// ---------------------------------------------------------------------------

fn spawn_chroma(binary: &PathBuf, persist_dir: &PathBuf, port: u16) -> Result<Child, SidecarError> {
    Command::new(binary)
        .arg("run")
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(port.to_string())
        .arg("--path")
        .arg(persist_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| SidecarError::SpawnFailed(e.to_string()))
}

#[cfg(unix)]
fn send_sigterm(child: &Child) {
    unsafe {
        libc::kill(child.id() as libc::pid_t, libc::SIGTERM);
    }
}

#[cfg(not(unix))]
fn send_sigterm(_child: &Child) {
    // On non-Unix platforms, SIGTERM is not available; kill() in stop will
    // handle cleanup.
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Starts a Chroma sidecar process. Blocks until the server is healthy or the
/// health-check deadline expires.
pub fn start_chroma(persist_dir: PathBuf, port: u16) -> Result<(), SidecarError> {
    let mut guard = SIDECAR.lock();

    // Prevent double-start.
    if let Some(ref mut sc) = *guard {
        if let Some(ref mut proc) = sc.process {
            match proc.try_wait() {
                Ok(None) => return Err(SidecarError::AlreadyRunning),
                _ => {
                    // Process has exited; fall through to restart logic.
                }
            }
        }
    }

    let binary = find_chroma_binary()?;
    let child = spawn_chroma(&binary, &persist_dir, port)?;

    let mut sidecar = ChromaSidecar {
        process: Some(child),
        persist_dir: persist_dir.clone(),
        port,
        started_at: Instant::now(),
        restart_count: 0,
    };

    // Drop the lock before polling health (which may block for up to 10s).
    // We temporarily take ownership and re-acquire afterwards.
    drop(guard);

    if !poll_health(port) {
        // Health check failed — try restarts with exponential backoff.
        let mut attempts = 0u32;
        let mut healthy = false;

        while attempts < MAX_RESTARTS {
            attempts += 1;

            // Exponential backoff: 1s, 2s, 4s
            let backoff = Duration::from_secs(1 << (attempts - 1));
            std::thread::sleep(backoff);

            // Kill the previous process if it is still lingering.
            if let Some(ref mut proc) = sidecar.process {
                let _ = proc.kill();
                let _ = proc.wait();
            }

            match spawn_chroma(&binary, &persist_dir, port) {
                Ok(child) => {
                    sidecar.process = Some(child);
                    sidecar.restart_count = attempts;
                    sidecar.started_at = Instant::now();

                    if poll_health(port) {
                        healthy = true;
                        break;
                    }
                }
                Err(_) => continue,
            }
        }

        if !healthy {
            // Clean up the last process.
            if let Some(ref mut proc) = sidecar.process {
                let _ = proc.kill();
                let _ = proc.wait();
            }
            return Err(SidecarError::MaxRestartsExceeded(MAX_RESTARTS));
        }
    }

    let mut guard = SIDECAR.lock();
    *guard = Some(sidecar);
    Ok(())
}

/// Stops the running Chroma sidecar process. Sends SIGTERM, waits up to 5
/// seconds, then SIGKILL.
pub fn stop_chroma() -> Result<(), SidecarError> {
    let mut guard = SIDECAR.lock();

    let sidecar = guard.as_mut().ok_or(SidecarError::NotRunning)?;
    let process = sidecar.process.as_mut().ok_or(SidecarError::NotRunning)?;

    // Send SIGTERM.
    send_sigterm(process);

    // Wait up to the grace period for graceful shutdown.
    let deadline = Instant::now() + STOP_GRACE_PERIOD;
    loop {
        match process.try_wait() {
            Ok(Some(_)) => {
                // Process exited cleanly.
                *guard = None;
                return Ok(());
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                *guard = None;
                return Err(SidecarError::StopFailed(e.to_string()));
            }
        }
    }

    // Grace period expired — force kill.
    process
        .kill()
        .map_err(|e| SidecarError::StopFailed(e.to_string()))?;
    let _ = process.wait();

    *guard = None;
    Ok(())
}

/// Returns `true` if the Chroma sidecar process is currently running.
pub fn is_chroma_running() -> bool {
    let mut guard = SIDECAR.lock();

    if let Some(ref mut sc) = *guard {
        if let Some(ref mut proc) = sc.process {
            match proc.try_wait() {
                Ok(None) => return true,  // Still running.
                Ok(Some(_)) => return false, // Exited.
                Err(_) => return false,
            }
        }
    }

    false
}
