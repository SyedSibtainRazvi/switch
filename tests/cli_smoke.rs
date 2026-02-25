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
    Command::new(env!("CARGO_BIN_EXE_context0"))
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("run context0")
}

fn git(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("run git")
}

fn git_stdout(cwd: &Path, args: &[&str]) -> String {
    let output = git(cwd, args);
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git stdout utf8")
        .trim()
        .to_string()
}

#[test]
fn cli_save_then_resume_json_round_trip() {
    if Command::new("git").arg("--version").output().is_err() {
        eprintln!("git is not available; skipping cli smoke test");
        return;
    }

    let root = temp_path("context0-cli-test");
    let repo = root.join("repo");
    let db = root.join("context0.db");
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
        "context0 save failed: {}",
        String::from_utf8_lossy(&save.stderr)
    );

    let resume = run_switch(
        &repo,
        &["--db", db.to_str().expect("db path"), "resume", "--json"],
    );
    assert!(
        resume.status.success(),
        "context0 resume failed: {}",
        String::from_utf8_lossy(&resume.stderr)
    );

    let payload: Value =
        serde_json::from_slice(&resume.stdout).expect("resume stdout should be valid json");
    assert_eq!(payload["done_text"], "cli test done");
    assert_eq!(payload["next_text"], "cli test next");
}

#[test]
fn cli_branch_override_uses_commit_from_that_branch() {
    if Command::new("git").arg("--version").output().is_err() {
        eprintln!("git is not available; skipping branch override smoke test");
        return;
    }

    let root = temp_path("context0-branch-override-test");
    let repo = root.join("repo");
    let db = root.join("context0.db");
    fs::create_dir_all(&repo).expect("create temp repo");

    let init = git(&repo, &["init"]);
    assert!(
        init.status.success(),
        "git init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    fs::write(repo.join("file.txt"), "main\n").expect("write file");
    let add_main = git(&repo, &["add", "file.txt"]);
    assert!(
        add_main.status.success(),
        "git add main failed: {}",
        String::from_utf8_lossy(&add_main.stderr)
    );
    let commit_main = git(
        &repo,
        &[
            "-c",
            "user.name=test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "main commit",
        ],
    );
    assert!(
        commit_main.status.success(),
        "git commit main failed: {}",
        String::from_utf8_lossy(&commit_main.stderr)
    );

    let create_feature = git(&repo, &["checkout", "-b", "feature/x"]);
    assert!(
        create_feature.status.success(),
        "git checkout feature failed: {}",
        String::from_utf8_lossy(&create_feature.stderr)
    );

    fs::write(repo.join("file.txt"), "feature\n").expect("write feature file");
    let add_feature = git(&repo, &["add", "file.txt"]);
    assert!(
        add_feature.status.success(),
        "git add feature failed: {}",
        String::from_utf8_lossy(&add_feature.stderr)
    );
    let commit_feature = git(
        &repo,
        &[
            "-c",
            "user.name=test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "feature commit",
        ],
    );
    assert!(
        commit_feature.status.success(),
        "git commit feature failed: {}",
        String::from_utf8_lossy(&commit_feature.stderr)
    );

    let feature_sha = git_stdout(&repo, &["rev-parse", "HEAD"]);

    let checkout_main = git(&repo, &["checkout", "main"]);
    assert!(
        checkout_main.status.success(),
        "git checkout main failed: {}",
        String::from_utf8_lossy(&checkout_main.stderr)
    );

    let save = run_switch(
        &repo,
        &[
            "--db",
            db.to_str().expect("db path"),
            "--branch",
            "feature/x",
            "save",
            "--done",
            "branch override test",
        ],
    );
    assert!(
        save.status.success(),
        "context0 save failed: {}",
        String::from_utf8_lossy(&save.stderr)
    );

    let resume = run_switch(
        &repo,
        &[
            "--db",
            db.to_str().expect("db path"),
            "--branch",
            "feature/x",
            "resume",
            "--json",
        ],
    );
    assert!(
        resume.status.success(),
        "context0 resume failed: {}",
        String::from_utf8_lossy(&resume.stderr)
    );

    let payload: Value =
        serde_json::from_slice(&resume.stdout).expect("resume stdout should be valid json");
    assert_eq!(payload["commit_sha"], feature_sha);
}
