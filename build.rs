use std::process::Command;

fn main() {
    // Auto-configure git hooks path on build so post-commit vanity hook works
    let _ = Command::new("git")
        .args(["config", "core.hooksPath", "hooks"])
        .status();
}
