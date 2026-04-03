use std::path::Path;
use std::process::Command;

fn binary() -> Command {
    let path = Path::new(env!("CARGO_BIN_EXE_git-vanity"));
    Command::new(path)
}

fn setup_temp_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .output()
            .unwrap()
    };
    run(&["init"]);
    run(&[
        "-c",
        "user.name=Test",
        "-c",
        "user.email=test@test.com",
        "commit",
        "--allow-empty",
        "-m",
        "initial commit",
    ]);
    dir
}

// --- CLI argument tests ---

#[test]
fn test_no_args_exits_1() {
    let dir = setup_temp_repo();
    let out = binary().current_dir(dir.path()).output().unwrap();
    // clap exits with 2 for missing required args
    assert_ne!(out.status.code(), Some(0));
}

#[test]
fn test_invalid_pattern_exits_1() {
    let dir = setup_temp_repo();
    let out = binary()
        .args(["xyz"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Invalid pattern"));
}

#[test]
fn test_not_a_repo_exits_3() {
    let dir = tempfile::tempdir().unwrap();
    let out = binary()
        .args(["cafe"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn test_max_attempts_exits_2() {
    let dir = setup_temp_repo();
    let out = binary()
        .args(["00000000", "--max-attempts", "10", "--dry-run"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn test_timeout_exits_2() {
    let dir = setup_temp_repo();
    let out = binary()
        .args(["0000000000", "--timeout", "50", "--dry-run"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

// --- Dry-run tests (no repo mutation) ---

#[test]
fn test_dry_run_prefix() {
    let dir = setup_temp_repo();
    let out = binary()
        .args(["cafe", "--dry-run"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("cafe"));
}

#[test]
fn test_dry_run_repeat() {
    let dir = setup_temp_repo();
    let out = binary()
        .args(["repeat:3", "--dry-run"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
}

#[test]
fn test_dry_run_pair() {
    let dir = setup_temp_repo();
    let out = binary()
        .args(["xx", "--dry-run"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
}

#[test]
fn test_dry_run_regex() {
    let dir = setup_temp_repo();
    let out = binary()
        .args(["/^dead/", "--dry-run"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("dead"));
}

#[test]
fn test_dry_run_structured() {
    let dir = setup_temp_repo();
    let out = binary()
        .args(["aaxxx", "--dry-run"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
}

#[test]
fn test_dry_run_structured_odd_prefix() {
    let dir = setup_temp_repo();
    // Use short odd prefix (3 chars) to keep test fast
    let out = binary()
        .args(["aax", "--dry-run"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("aa"));
}

// --- Preset tests ---

#[test]
fn test_preset_cafe() {
    let dir = setup_temp_repo();
    let out = binary()
        .args(["-p", "cafe", "--dry-run"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("cafe"));
}

#[test]
fn test_preset_unknown() {
    let dir = setup_temp_repo();
    let out = binary()
        .args(["-p", "banana"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Unknown preset"));
}

#[test]
fn test_list_presets() {
    let out = binary().args(["--list-presets"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("cafe"));
    assert!(stdout.contains("dead"));
    assert!(stdout.contains("c0ffee"));
}

// --- Write tests (actually mutates repo) ---

#[test]
fn test_write_updates_head() {
    let dir = setup_temp_repo();

    let old_head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let old_hash = String::from_utf8_lossy(&old_head.stdout).trim().to_string();

    let out = binary()
        .args(["cafe"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());

    let new_head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let new_hash = String::from_utf8_lossy(&new_head.stdout).trim().to_string();

    assert_ne!(old_hash, new_hash);
    assert!(new_hash.starts_with("cafe"));
}

#[test]
fn test_write_preserves_message() {
    let dir = setup_temp_repo();

    let out = binary()
        .args(["cafe"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());

    let log = Command::new("git")
        .args(["log", "--format=%s", "-1"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let msg = String::from_utf8_lossy(&log.stdout).trim().to_string();
    assert_eq!(msg, "initial commit");
}

#[test]
fn test_write_passes_fsck() {
    let dir = setup_temp_repo();

    binary()
        .args(["cafe"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    let fsck = Command::new("git")
        .args(["fsck"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(fsck.status.success());
}

#[test]
fn test_idempotent_rerun() {
    let dir = setup_temp_repo();

    // Run twice — second run should re-read the nonce-containing commit
    binary()
        .args(["cafe"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    let out = binary()
        .args(["dead"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());

    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let hash = String::from_utf8_lossy(&head.stdout).trim().to_string();
    assert!(hash.starts_with("dead"));
}

#[test]
fn test_message_override() {
    let dir = setup_temp_repo();

    let out = binary()
        .args(["cafe", "--message", "custom message"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());

    let log = Command::new("git")
        .args(["log", "--format=%s", "-1"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let msg = String::from_utf8_lossy(&log.stdout).trim().to_string();
    assert_eq!(msg, "custom message");
}

// --- Thread option ---

#[test]
fn test_single_thread() {
    let dir = setup_temp_repo();
    let out = binary()
        .args(["cafe", "--dry-run", "-j", "1"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
}

// --- Debug output ---

#[test]
fn test_debug_output() {
    let dir = setup_temp_repo();
    let out = binary()
        .args(["cafe", "--dry-run", "--debug"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("[vanity] threads:"));
    assert!(stderr.contains("[vanity] pattern:"));
    assert!(stderr.contains("[vanity] match:"));
}

// --- No-repeat flag ---

#[test]
fn test_no_repeat_rejects_structured() {
    let dir = setup_temp_repo();
    let out = binary()
        .args(["1997xxx", "--no-repeat"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn test_version() {
    let out = binary().args(["-V"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("git-vanity"));
}

#[test]
fn test_empty_repo_no_head() {
    let dir = tempfile::tempdir().unwrap();
    Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    // No commits — should exit 3
    let out = binary()
        .args(["cafe"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn test_message_with_trailing_newline() {
    let dir = setup_temp_repo();
    let out = binary()
        .args(["cafe", "--message", "msg with newline\n"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());

    let log = Command::new("git")
        .args(["log", "--format=%s", "-1"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let msg = String::from_utf8_lossy(&log.stdout).trim().to_string();
    assert_eq!(msg, "msg with newline");
}

#[test]
fn test_preset_overrides_pattern() {
    let dir = setup_temp_repo();
    // When both preset and pattern given, preset wins
    let out = binary()
        .args(["-p", "dead", "cafe", "--dry-run"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("dead"));
}

// --- Match position tests ---

#[test]
fn test_match_start_default() {
    let dir = setup_temp_repo();
    let out = binary()
        .args(["cafe", "--dry-run"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Hash in output should start with cafe
    assert!(stdout.contains("cafe"));
}

#[test]
fn test_match_end() {
    let dir = setup_temp_repo();
    let out = binary()
        .args(["cafe", "-m", "end", "--dry-run"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("cafe"));
}

#[test]
fn test_match_contains() {
    let dir = setup_temp_repo();
    let out = binary()
        .args(["cafe", "-m", "contains", "--dry-run"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("cafe"));
}

#[test]
fn test_match_invalid_position() {
    let dir = setup_temp_repo();
    let out = binary()
        .args(["cafe", "-m", "middle"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn test_match_end_debug() {
    let dir = setup_temp_repo();
    let out = binary()
        .args(["cafe", "-m", "end", "--dry-run", "--debug"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("end"));
}

// --- Show command ---

#[test]
fn test_show_with_vanity() {
    let dir = setup_temp_repo();
    // Apply vanity first
    binary()
        .args(["cafe"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    let out = binary()
        .args(["show"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Vanity: yes"));
    assert!(stdout.contains("cafe"));
}

#[test]
fn test_show_without_vanity() {
    let dir = setup_temp_repo();
    let out = binary()
        .args(["show"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Vanity: no"));
}

#[test]
fn test_log_command() {
    let dir = setup_temp_repo();
    binary()
        .args(["cafe"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    let out = binary()
        .args(["log"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("1/1 commits have vanity hashes"));
}

#[test]
fn test_log_no_vanity_commits() {
    let dir = setup_temp_repo();
    // No vanity applied — commit should not be counted
    let out = binary()
        .args(["log"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("0/1 commits have vanity hashes"));
    // No checkmark for plain commit
    assert!(!stdout.contains("\u{2713}"));
}

#[test]
fn test_log_mixed_commits() {
    let dir = setup_temp_repo();
    // First commit is plain (no vanity)
    // Add a second commit with vanity
    Command::new("git")
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@test.com",
            "commit",
            "--allow-empty",
            "-m",
            "second commit",
        ])
        .current_dir(dir.path())
        .output()
        .unwrap();

    binary()
        .args(["cafe"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    let out = binary()
        .args(["log"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Only the vanity commit counts, not the plain one
    assert!(stdout.contains("1/2 commits have vanity hashes"));
}

#[test]
fn test_log_validates_hash_pattern() {
    let dir = setup_temp_repo();
    // Apply vanity with recognizable pattern
    binary()
        .args(["cafe"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    // Verify the hash in log output starts with cafe
    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let hash = String::from_utf8_lossy(&head.stdout).trim().to_string();
    assert!(hash.starts_with("cafe"));

    let out = binary()
        .args(["log"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Should have checkmark because hash AND nonce are valid
    assert!(stdout.contains("\u{2713}"));
    assert!(stdout.contains("1/1 commits have vanity hashes"));
}

#[test]
fn test_log_multiple_vanity() {
    let dir = setup_temp_repo();

    binary()
        .args(["dead"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    Command::new("git")
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@test.com",
            "commit",
            "--allow-empty",
            "-m",
            "second",
        ])
        .current_dir(dir.path())
        .output()
        .unwrap();

    binary()
        .args(["beef"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    let out = binary()
        .args(["log"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("2/2 commits have vanity hashes"));
}

// --- Undo tests ---

#[test]
fn test_undo_removes_nonce() {
    let dir = setup_temp_repo();

    // Apply vanity
    binary()
        .args(["cafe"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    let head_before = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let hash_before = String::from_utf8_lossy(&head_before.stdout)
        .trim()
        .to_string();
    assert!(hash_before.starts_with("cafe"));

    // Undo
    let out = binary()
        .args(["undo"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("nonce removed"));

    // Hash should change
    let head_after = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let hash_after = String::from_utf8_lossy(&head_after.stdout)
        .trim()
        .to_string();
    assert_ne!(hash_before, hash_after);

    // Should pass fsck
    let fsck = Command::new("git")
        .args(["fsck"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(fsck.status.success());
}

#[test]
fn test_undo_no_nonce() {
    let dir = setup_temp_repo();
    let out = binary()
        .args(["undo"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("nothing to undo"));
}

#[test]
fn test_no_pattern_no_preset_errors() {
    let dir = setup_temp_repo();
    let out = binary().current_dir(dir.path()).output().unwrap();
    assert_ne!(out.status.code(), Some(0));
}
