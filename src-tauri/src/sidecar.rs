use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::process::{Child, Command};
use std::time::{Duration, Instant};
use thiserror::Error;

use crate::chroma;
use crate::config;
use crate::grpc_client;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const PYTHON_DEFAULT_PORT: u16 = 50051;
const FIRST_START_HEALTH_DEADLINE: Duration = Duration::from_secs(30);
const RESTART_HEALTH_DEADLINE: Duration = Duration::from_secs(10);
const MAX_RESTART_ATTEMPTS: u32 = 3;
const SIGTERM_GRACE_PERIOD: Duration = Duration::from_secs(5);
const HEALTH_POLL_INTERVAL: Duration = Duration::from_millis(500);

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum SidecarError {
    #[error("Python not found — install python3 and ensure it is on PATH")]
    PythonNotFound,
    #[error("Failed to spawn Python sidecar: {0}")]
    SpawnFailed(String),
    #[error("Python sidecar health check timed out after {0}s")]
    HealthTimeout(u64),
    #[error("Python sidecar exited unexpectedly")]
    ProcessExited,
    #[error("Chroma error: {0}")]
    Chroma(String),
    #[error("gRPC error: {0}")]
    Grpc(String),
    #[error("Max restart attempts ({0}) exceeded")]
    MaxRestartsExceeded(u32),
    #[error("IO error: {0}")]
    Io(String),
    #[error("Config error: {0}")]
    Config(String),
}

impl Serialize for SidecarError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<std::io::Error> for SidecarError {
    fn from(err: std::io::Error) -> Self {
        SidecarError::Io(err.to_string())
    }
}

impl From<grpc_client::GrpcError> for SidecarError {
    fn from(err: grpc_client::GrpcError) -> Self {
        SidecarError::Grpc(err.to_string())
    }
}

impl From<config::ConfigError> for SidecarError {
    fn from(err: config::ConfigError) -> Self {
        SidecarError::Config(err.to_string())
    }
}

// ---------------------------------------------------------------------------
// Status type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarStatus {
    pub chroma_running: bool,
    pub chroma_port: u16,
    pub python_running: bool,
    pub python_port: u16,
    pub python_model_ready: bool,
}

// ---------------------------------------------------------------------------
// Python sidecar state
// ---------------------------------------------------------------------------

struct PythonSidecar {
    process: Option<Child>,
    port: u16,
    started_at: Instant,
    restart_count: u32,
}

static PYTHON_SIDECAR: Mutex<Option<PythonSidecar>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// Python helpers
// ---------------------------------------------------------------------------

/// Resolve a working Python interpreter, preferring `python3` over `python`.
fn resolve_python() -> Result<String, SidecarError> {
    for candidate in &["python3", "python"] {
        let result = Command::new("which")
            .arg(candidate)
            .output();

        if let Ok(output) = result {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    tracing::debug!("Resolved Python interpreter: {path}");
                    return Ok(path);
                }
            }
        }
    }

    Err(SidecarError::PythonNotFound)
}

/// Spawn the Python sidecar process. The script path is resolved relative to
/// the application resource directory (for bundled builds) or falls back to a
/// path relative to the Cargo workspace root during development.
fn spawn_python_sidecar(port: u16) -> Result<Child, SidecarError> {
    let python = resolve_python()?;

    // In development the sidecar script lives at `<project>/apps/ingestion/sidecar/server.py`
    // relative to the Tauri src dir. At runtime, Tauri bundles resources into the
    // app's resource directory. We try the resource path first, then fall back to
    // the relative dev path.
    let script_path = "sidecar/server.py";

    tracing::info!(
        "Spawning Python sidecar: {python} {script_path} --port {port}"
    );

    let child = Command::new(&python)
        .arg(script_path)
        .arg("--port")
        .arg(port.to_string())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| SidecarError::SpawnFailed(e.to_string()))?;

    Ok(child)
}

/// Poll the gRPC health endpoint until it reports ready or the deadline
/// expires.
fn wait_for_python_health(deadline: Duration) -> Result<(), SidecarError> {
    let start = Instant::now();

    loop {
        if start.elapsed() >= deadline {
            return Err(SidecarError::HealthTimeout(deadline.as_secs()));
        }

        // Check if the process is still alive.
        {
            let mut guard = PYTHON_SIDECAR.lock();
            if let Some(ref mut sidecar) = *guard {
                if let Some(ref mut child) = sidecar.process {
                    match child.try_wait() {
                        Ok(Some(_status)) => {
                            return Err(SidecarError::ProcessExited);
                        }
                        Ok(None) => { /* still running */ }
                        Err(e) => {
                            tracing::warn!("Error checking sidecar process: {e}");
                        }
                    }
                }
            }
        }

        // Attempt a blocking health check via the tokio runtime.
        // Uses Handle::current().block_on() directly — safe on a spawn_blocking thread.
        let health_result = tokio::runtime::Handle::current().block_on(async {
            let client = grpc_client::get_grpc_client()?;
            client.health().await
        });

        match health_result {
            Ok(info) if info.ready => {
                tracing::info!(
                    "Python sidecar ready: model={}, dimension={}",
                    info.model_name,
                    info.dimension
                );
                return Ok(());
            }
            Ok(_) => {
                tracing::debug!("Sidecar responded but model not ready yet");
            }
            Err(e) => {
                tracing::debug!("Health poll attempt failed: {e}");
            }
        }

        std::thread::sleep(HEALTH_POLL_INTERVAL);
    }
}

/// Start the Python sidecar and wait for it to become healthy.
fn start_python(port: u16, is_restart: bool) -> Result<(), SidecarError> {
    let child = spawn_python_sidecar(port)?;

    {
        let mut guard = PYTHON_SIDECAR.lock();
        let restart_count = guard
            .as_ref()
            .map(|s| s.restart_count)
            .unwrap_or(0);

        *guard = Some(PythonSidecar {
            process: Some(child),
            port,
            started_at: Instant::now(),
            restart_count: if is_restart { restart_count + 1 } else { 0 },
        });
    }

    // Initialize the gRPC client for this port.
    grpc_client::init_grpc_client(port);

    let deadline = if is_restart {
        RESTART_HEALTH_DEADLINE
    } else {
        FIRST_START_HEALTH_DEADLINE
    };

    wait_for_python_health(deadline)
}

/// Stop the Python sidecar process gracefully (SIGTERM, then SIGKILL).
fn stop_python() -> Result<(), SidecarError> {
    let mut guard = PYTHON_SIDECAR.lock();

    if let Some(ref mut sidecar) = *guard {
        if let Some(ref mut child) = sidecar.process {
            let pid = child.id();
            tracing::info!("Stopping Python sidecar (pid={pid})");

            // Send SIGTERM on Unix.
            #[cfg(unix)]
            {
                unsafe {
                    libc::kill(pid as i32, libc::SIGTERM);
                }
            }

            // On non-Unix, fall back to kill() directly.
            #[cfg(not(unix))]
            {
                let _ = child.kill();
            }

            // Wait up to the grace period for clean shutdown.
            let start = Instant::now();
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        tracing::info!("Python sidecar exited: {status}");
                        break;
                    }
                    Ok(None) => {
                        if start.elapsed() >= SIGTERM_GRACE_PERIOD {
                            tracing::warn!(
                                "Python sidecar did not exit within {}s, sending SIGKILL",
                                SIGTERM_GRACE_PERIOD.as_secs()
                            );
                            let _ = child.kill();
                            let _ = child.wait();
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(100));
                    }
                    Err(e) => {
                        tracing::error!("Error waiting for sidecar process: {e}");
                        let _ = child.kill();
                        break;
                    }
                }
            }
        }
    }

    *guard = None;
    grpc_client::reset_grpc_client();

    Ok(())
}

/// Check whether the Python sidecar process is still alive.
fn is_python_running() -> bool {
    let mut guard = PYTHON_SIDECAR.lock();
    if let Some(ref mut sidecar) = *guard {
        if let Some(ref mut child) = sidecar.process {
            match child.try_wait() {
                Ok(Some(_)) => false, // exited
                Ok(None) => true,     // still running
                Err(_) => false,
            }
        } else {
            false
        }
    } else {
        false
    }
}

/// Check whether the Python model is ready via the gRPC health endpoint.
///
/// Uses `Handle::current().block_on()` directly, which is safe when called
/// from a `spawn_blocking` thread (the blocking pool).
fn is_python_model_ready() -> bool {
    let result = tokio::runtime::Handle::current().block_on(async {
        let client = grpc_client::get_grpc_client()?;
        client.health().await
    });

    matches!(result, Ok(info) if info.ready)
}

// ---------------------------------------------------------------------------
// Chroma helpers — delegates to chroma::sidecar which tracks the child process
// ---------------------------------------------------------------------------

/// Start the Chroma DB sidecar process via the proper ChromaSidecar manager.
fn start_chroma(port: u16) -> Result<(), SidecarError> {
    let persist_dir = config::chroma_persist_dir()
        .map_err(|e| SidecarError::Config(e.to_string()))?;

    match chroma::sidecar::start_chroma(persist_dir, port) {
        Ok(()) => {
            tracing::info!("Chroma is running on port {port}");
            Ok(())
        }
        Err(chroma::sidecar::SidecarError::AlreadyRunning) => {
            tracing::debug!("Chroma already running on port {port}");
            Ok(())
        }
        Err(chroma::sidecar::SidecarError::BinaryNotFound) => {
            // If the `chroma` CLI is not found, assume Chroma is managed
            // externally (e.g., already running or started via Docker).
            tracing::warn!(
                "Chroma binary not found. Assuming Chroma is managed externally."
            );
            Ok(())
        }
        Err(e) => Err(SidecarError::Chroma(e.to_string())),
    }
}

/// Check whether Chroma is reachable by hitting the heartbeat endpoint.
///
/// Uses `Handle::current().block_on()` directly, which is safe when called
/// from a `spawn_blocking` thread (the blocking pool).
fn is_chroma_running(port: u16) -> bool {
    let client = chroma::client::get_client_with_port(port);
    let result = tokio::runtime::Handle::current()
        .block_on(async { client.heartbeat().await });

    result.is_ok()
}

// ---------------------------------------------------------------------------
// Public API — orchestration
// ---------------------------------------------------------------------------

/// Start both Chroma and the Python sidecar, waiting for each to become
/// healthy before returning.
pub fn start_all() -> Result<SidecarStatus, SidecarError> {
    let app_config = config::load_config()?;
    let chroma_port = app_config.chroma_port;
    let python_port = app_config.sidecar_port;

    // 1. Start Chroma first.
    start_chroma(chroma_port)?;

    // 2. Start Python sidecar (with retry logic).
    let mut last_error: Option<SidecarError> = None;

    for attempt in 0..=MAX_RESTART_ATTEMPTS {
        let is_restart = attempt > 0;

        if is_restart {
            // Exponential backoff: 1s, 2s, 4s
            let backoff = Duration::from_secs(1 << (attempt - 1));
            tracing::warn!(
                "Restarting Python sidecar (attempt {attempt}/{MAX_RESTART_ATTEMPTS}), \
                 backoff {}s",
                backoff.as_secs()
            );
            std::thread::sleep(backoff);
        }

        match start_python(python_port, is_restart) {
            Ok(()) => {
                return Ok(build_status(chroma_port, python_port));
            }
            Err(e) => {
                tracing::error!("Python sidecar start failed: {e}");
                // Clean up the failed process before retrying.
                let _ = stop_python();
                last_error = Some(e);
            }
        }
    }

    Err(last_error.unwrap_or(SidecarError::MaxRestartsExceeded(MAX_RESTART_ATTEMPTS)))
}

/// Stop both sidecars. Python is stopped first so it can flush any pending
/// writes to Chroma before Chroma shuts down.
pub fn stop_all() -> Result<(), SidecarError> {
    tracing::info!("Stopping all sidecars");

    // 1. Stop Python first.
    stop_python()?;

    // 2. Stop Chroma (best-effort — it may be externally managed).
    let _ = chroma::sidecar::stop_chroma();
    chroma::client::reset_client();

    tracing::info!("All sidecars stopped");
    Ok(())
}

/// Build a `SidecarStatus` snapshot from the current state.
fn build_status(chroma_port: u16, python_port: u16) -> SidecarStatus {
    SidecarStatus {
        chroma_running: is_chroma_running(chroma_port),
        chroma_port,
        python_running: is_python_running(),
        python_port,
        python_model_ready: if is_python_running() {
            is_python_model_ready()
        } else {
            false
        },
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn sidecar_start_all() -> Result<SidecarStatus, SidecarError> {
    tokio::task::spawn_blocking(start_all)
        .await
        .map_err(|e| SidecarError::Io(e.to_string()))?
}

#[tauri::command]
pub async fn sidecar_stop_all() -> Result<(), SidecarError> {
    tokio::task::spawn_blocking(stop_all)
        .await
        .map_err(|e| SidecarError::Io(e.to_string()))?
}

#[tauri::command]
pub async fn sidecar_status() -> Result<SidecarStatus, SidecarError> {
    tokio::task::spawn_blocking(|| {
        let app_config = config::load_config()?;
        Ok(build_status(app_config.chroma_port, app_config.sidecar_port))
    })
    .await
    .map_err(|e| SidecarError::Io(e.to_string()))?
}
