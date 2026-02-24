use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_path(prefix: &str) -> PathBuf {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_millis();
    std::env::temp_dir().join(format!("{prefix}-{}-{now_ms}", std::process::id()))
}

fn run_switch(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_switch"))
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("run switch")
}

#[test]
fn cli_save_then_resume_json_round_trip() {
    if Command::new("git").arg("--version").output().is_err() {
        eprintln!("git is not available; skipping cli smoke test");
        return;
    }

    let root = temp_path("switch-cli-test");
    let repo = root.join("repo");
    let db = root.join("switch.db");
    fs::create_dir_all(&repo).expect("create temp repo");

    let init = Command::new("git")
        .arg("init")
        .current_dir(&repo)
        .output()
        .expect("git init");
    assert!(
        init.status.success(),
        "git init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let save = run_switch(
        &repo,
        &[
            "--db",
            db.to_str().expect("db path"),
            "save",
            "--done",
            "cli test done",
            "--next",
            "cli test next",
            "--files",
            "src/main.rs",
        ],
    );
    assert!(
        save.status.success(),
        "switch save failed: {}",
        String::from_utf8_lossy(&save.stderr)
    );

    let resume = run_switch(
        &repo,
        &["--db", db.to_str().expect("db path"), "resume", "--json"],
    );
    assert!(
        resume.status.success(),
        "switch resume failed: {}",
        String::from_utf8_lossy(&resume.stderr)
    );

    let payload: Value =
        serde_json::from_slice(&resume.stdout).expect("resume stdout should be valid json");
    assert_eq!(payload["done_text"], "cli test done");
    assert_eq!(payload["next_text"], "cli test next");
}
