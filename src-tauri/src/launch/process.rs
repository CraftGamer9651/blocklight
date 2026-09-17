use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::error::AppError;

use super::args::BuiltCommand;
use super::java::DetectedJava;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameLogLine {
    pub instance_id: String,
    pub stream: &'static str,
    pub line: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameExited {
    pub instance_id: String,
    pub exit_code: Option<i32>,
}

/// Tracks the OS process id of every running instance, purely so
/// `stop` can ask the OS to terminate it. Log streaming and exit
/// detection happen in a detached task started by `spawn` below, which
/// owns the actual `Child` handle for its whole lifetime -- keeping a
/// `Child` in a shared, lockable registry doesn't work cleanly, since
/// whatever holds it for `.wait().await` would have to hold a lock for
/// the entire time the game is running, blocking `stop` from ever
/// getting in. Termination-by-PID sidesteps that entirely.
pub struct RunningProcesses {
    pids: Mutex<HashMap<String, u32>>,
}

impl RunningProcesses {
    pub fn new() -> Self {
        RunningProcesses {
            pids: Mutex::new(HashMap::new()),
        }
    }

    pub fn is_running(&self, instance_id: &str) -> bool {
        self.pids.lock().unwrap().contains_key(instance_id)
    }

    pub fn stop(&self, instance_id: &str) -> Result<(), AppError> {
        let pid = self.pids.lock().unwrap().get(instance_id).copied();
        match pid {
            Some(pid) => kill_pid(pid),
            None => Ok(()),
        }
    }
}

impl Default for RunningProcesses {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(unix)]
fn kill_pid(pid: u32) -> Result<(), AppError> {
    let pid_str = pid.to_string();
    std::process::Command::new("kill").args(["-9", pid_str.as_str()]).status()?;
    Ok(())
}

#[cfg(windows)]
fn kill_pid(pid: u32) -> Result<(), AppError> {
    let pid_str = pid.to_string();
    std::process::Command::new("taskkill")
        .args(["/F", "/PID", pid_str.as_str()])
        .status()?;
    Ok(())
}

/// Spawns the game process and returns immediately -- it does not wait
/// for the game to exit. Stdout/stderr are streamed line-by-line to the
/// frontend as `game-log` events, and a `game-exited` event fires once
/// the process terminates (by itself or via `stop`).
pub fn spawn(
    app: AppHandle,
    registry: Arc<RunningProcesses>,
    instance_id: String,
    java: &DetectedJava,
    command: BuiltCommand,
) -> Result<(), AppError> {
    let mut cmd = Command::new(&java.executable);
    cmd.args(&command.java_args)
        .current_dir(&command.working_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn()?;
    let pid = child.id().unwrap_or(0);
    registry.pids.lock().unwrap().insert(instance_id.clone(), pid);

    if let Some(stdout) = child.stdout.take() {
        let app = app.clone();
        let instance_id = instance_id.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = app.emit(
                    "game-log",
                    GameLogLine {
                        instance_id: instance_id.clone(),
                        stream: "stdout",
                        line,
                    },
                );
            }
        });
    }

    if let Some(stderr) = child.stderr.take() {
        let app = app.clone();
        let instance_id = instance_id.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = app.emit(
                    "game-log",
                    GameLogLine {
                        instance_id: instance_id.clone(),
                        stream: "stderr",
                        line,
                    },
                );
            }
        });
    }

    tokio::spawn(async move {
        let status = child.wait().await.ok();
        registry.pids.lock().unwrap().remove(&instance_id);
        let _ = app.emit(
            "game-exited",
            GameExited {
                instance_id,
                exit_code: status.and_then(|s| s.code()),
            },
        );
    });

    Ok(())
}
