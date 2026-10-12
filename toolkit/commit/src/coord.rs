//! Bounded `ai-coord` subprocess calls. Coordination is advisory: a missing, slow, or failing
//! `ai-coord` never blocks preparation. Finding resolution is the one explicit request that reports
//! its failure.

use std::{
    env,
    io::Read,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use wait_timeout::ChildExt;

use crate::git::clear_repository_environment;

pub(crate) const BASELINE_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const TRAILER_TIMEOUT: Duration = Duration::from_millis(750);
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(5);
const RESOLVE_OUTPUT_LIMIT: u64 = 16 * 1024;

pub(crate) struct Output {
    pub success: bool,
    pub stdout: Vec<u8>,
    pub stderr: String,
}

/// Runs `ai-coord` and returns its stdout only on success within the limit and timeout.
pub(crate) fn bounded(repository_root: &Path, subcommand: &str, limit: u64, timeout: Duration) -> Option<Vec<u8>> {
    let output = invoke(repository_root, &[subcommand], limit, timeout)?;
    output.success.then_some(output.stdout)
}

/// Records `oid` as the fixed-resolution evidence of finding `id`.
pub(crate) fn resolve_finding(repository_root: &Path, id: &str, oid: &str) -> std::result::Result<(), String> {
    let arguments = ["finding", "resolve", id, "--as", "fixed", "--commit", oid];
    match invoke(repository_root, &arguments, RESOLVE_OUTPUT_LIMIT, RESOLVE_TIMEOUT) {
        Some(output) if output.success => Ok(()),
        Some(output) => {
            let detail = output.stderr.trim();
            Err(if detail.is_empty() { "ai-coord finding resolve failed".to_owned() } else { detail.to_owned() })
        }
        None => Err("ai-coord finding resolve is unavailable, timed out, or exceeded its output limit".to_owned()),
    }
}

fn invoke(repository_root: &Path, arguments: &[&str], limit: u64, timeout: Duration) -> Option<Output> {
    let timeout = env::var("AI_COMMIT_TEST_COORD_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|milliseconds| *milliseconds > 0)
        .map(Duration::from_millis)
        .unwrap_or(timeout);
    let mut command = Command::new("ai-coord");
    clear_repository_environment(&mut command);
    let mut child = command
        .args(arguments)
        .current_dir(repository_root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let stderr = child.stderr.take()?;
    let stdout_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.take(limit + 1).read_to_end(&mut bytes).ok()?;
        Some(bytes)
    });
    let stderr_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.take(limit + 1).read_to_end(&mut bytes).ok()?;
        Some(String::from_utf8_lossy(&bytes).into_owned())
    });
    let status = match child.wait_timeout(timeout).ok()? {
        Some(status) => status,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
    };
    let stdout = stdout_reader.join().ok()??;
    let stderr = stderr_reader.join().ok()??;
    if stdout.len() as u64 > limit {
        return None;
    }
    Some(Output { success: status.success(), stdout, stderr })
}
