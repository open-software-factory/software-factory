//! A child process's whole tree, killed on a timeout. Shared by `moon.rs`
//! and `reviewers.rs`, the two places that spawn a process that might hang
//! past a caller-given deadline.

use std::process::{Child, Command};

/// Kills `child` and every process it spawned: `taskkill /PID <pid> /T /F`
/// on Windows, `kill -KILL` on the process group on Unix (no new crate).
/// The caller is responsible for starting `child` as its own process-group
/// leader on Unix (`Command::process_group(0)`), otherwise the `kill -KILL`
/// below reaches only this one process, not its children.
///
/// # Errors
/// Returns the tree-kill command's own stderr, or its spawn error, when it
/// does not report success.
pub fn kill_tree(child: &Child) -> Result<(), String> {
    let pid = child.id();
    #[cfg(windows)]
    let output = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .output();
    #[cfg(unix)]
    let output = Command::new("kill")
        .args(["-KILL", "--", &format!("-{pid}")])
        .output();
    match output {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).trim().to_string()),
        Err(e) => Err(e.to_string()),
    }
}
