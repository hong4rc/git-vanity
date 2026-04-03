use std::process::Command;

/// Run a git command and return stdout on success.
/// Higher-order helper: eliminates boilerplate across all git operations.
fn run_git(args: &[&str]) -> Result<Vec<u8>, String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run git {}: {}", args[0], e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git {} failed: {}", args[0], stderr.trim()));
    }

    Ok(output.stdout)
}

/// Run a git command and return stdout as a trimmed string.
fn run_git_string(args: &[&str]) -> Result<String, String> {
    run_git(args).map(|out| String::from_utf8_lossy(&out).trim().to_string())
}

/// Run a git command, return true if it succeeds.
fn git_succeeds(args: &[&str]) -> bool {
    Command::new("git")
        .args(args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Validate git environment: must be a repo with HEAD.
pub fn ensure_repo() -> Result<(), String> {
    if !git_succeeds(&["rev-parse", "--git-dir"]) {
        return Err("not a git repository".into());
    }
    if !git_succeeds(&["rev-parse", "HEAD"]) {
        return Err("no HEAD commit (empty repository?)".into());
    }
    Ok(())
}

/// Read the raw commit object at HEAD.
pub fn read_head_commit() -> Result<String, String> {
    // Use lossy conversion: the nonce from a previous run contains non-UTF8 bytes
    // (0x80-0xFF range), but CommitObject::parse strips x-nonce lines anyway.
    run_git(&["cat-file", "commit", "HEAD"]).map(|out| String::from_utf8_lossy(&out).into_owned())
}

/// Write a raw commit object to Git's object store.
/// Returns the hash of the written object.
pub fn write_commit_object(content: &[u8]) -> Result<String, String> {
    let mut child = Command::new("git")
        .args(["hash-object", "-t", "commit", "-w", "--stdin"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn git: {}", e))?;

    {
        use std::io::Write;
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(content)
            .map_err(|e| format!("Failed to write to git stdin: {}", e))?;
    }

    child
        .wait_with_output()
        .map_err(|e| format!("Failed to wait for git: {}", e))
        .and_then(|output| {
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                Err(format!(
                    "git hash-object failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ))
            }
        })
}

/// Update HEAD to point to a new commit hash.
pub fn update_head(hash: &str) -> Result<(), String> {
    run_git(&["update-ref", "HEAD", hash]).map(|_| ())
}

/// Get current HEAD hash.
pub fn get_head_hash() -> Result<String, String> {
    run_git_string(&["rev-parse", "HEAD"])
}

/// Get commit log: returns Vec of (hash, has_nonce, subject).
pub fn log_with_nonce_info(max: usize) -> Result<Vec<(String, bool, String)>, String> {
    let hashes_output = run_git_string(&["log", "--format=%H", &format!("-{}", max)])?;
    let subjects_output = run_git_string(&["log", "--format=%s", &format!("-{}", max)])?;

    hashes_output
        .lines()
        .zip(subjects_output.lines())
        .map(|(hash, subject)| {
            let raw = run_git(&["cat-file", "commit", hash])
                .map(|out| String::from_utf8_lossy(&out).into_owned())?;
            let has_nonce = raw.lines().any(|l| l.starts_with("x-nonce "));
            Ok((hash.to_string(), has_nonce, subject.to_string()))
        })
        .collect()
}
