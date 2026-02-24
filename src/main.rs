use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use clap::{ArgAction, Parser, Subcommand};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Parser)]
#[command(
    name = "switch",
    version,
    about = "Local-first context broker for coding agents"
)]
struct Cli {
    /// Override sqlite db path (default: ~/.switch/switch.db)
    #[arg(long, global = true)]
    db: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Ensure db exists and migrations are applied
    Init,
    /// Save a checkpoint for current repo + branch
    Save {
        #[arg(long)]
        done: Option<String>,
        #[arg(long)]
        next: Option<String>,
        #[arg(long)]
        blockers: Option<String>,
        #[arg(long)]
        tests: Option<String>,
        #[arg(long, action = ArgAction::Append)]
        files: Vec<String>,
        #[arg(long)]
        session: Option<String>,
    },
    /// Show latest checkpoint for current repo + branch
    Resume {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Show recent checkpoints for current repo + branch
    Log {
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
}

#[derive(Debug, Serialize)]
struct Checkpoint {
    id: i64,
    repo_path: String,
    branch: String,
    commit_sha: String,
    session_id: Option<String>,
    done_text: Option<String>,
    next_text: Option<String>,
    blockers_text: Option<String>,
    tests_text: Option<String>,
    files: Vec<String>,
    created_at_ms: i64,
}

#[derive(Debug, Clone)]
struct ContextScope {
    repo_path: String,
    branch: String,
    commit_sha: String,
    used_repo_fallback: bool,
    used_branch_fallback: bool,
    used_commit_fallback: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let db_path = cli.db.unwrap_or_else(default_db_path);
    let conn = open_db(&db_path)?;

    match cli.command {
        Commands::Init => {
            println!("Initialized database at {}", db_path.display());
        }
        Commands::Save {
            done,
            next,
            blockers,
            tests,
            files,
            session,
        } => {
            let scope = detect_scope()?;
            warn_scope_fallback(&scope);
            save_checkpoint(&conn, &scope, done, next, blockers, tests, files, session)?;
            println!("Checkpoint saved");
        }
        Commands::Resume { json } => {
            let scope = detect_scope()?;
            warn_scope_fallback(&scope);
            if let Some(checkpoint) = latest_checkpoint_for_scope(&conn, &scope.repo_path, &scope.branch)? {
                if json {
                    println!("{}", serde_json::to_string_pretty(&checkpoint)?);
                } else {
                    print_checkpoint(&checkpoint);
                }
            } else {
                println!("No context found for this repo/branch.");
            }
        }
        Commands::Log { limit } => {
            let scope = detect_scope()?;
            warn_scope_fallback(&scope);
            let rows = list_checkpoints_for_scope(&conn, &scope.repo_path, &scope.branch, limit)?;
            if rows.is_empty() {
                println!("No checkpoints found.");
            } else {
                for row in rows {
                    print_checkpoint_compact(&row);
                }
            }
        }
    }

    Ok(())
}

fn default_db_path() -> PathBuf {
    match dirs::home_dir() {
        Some(home) => home.join(".switch").join("switch.db"),
        None => PathBuf::from(".switch/switch.db"),
    }
}

fn open_db(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create db dir {}", parent.display()))?;
    }

    let conn = Connection::open(path)
        .with_context(|| format!("failed to open sqlite db at {}", path.display()))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.execute_batch(include_str!("../migrations/0001_init.sql"))?;
    Ok(conn)
}

fn save_checkpoint(
    conn: &Connection,
    scope: &ContextScope,
    done: Option<String>,
    next: Option<String>,
    blockers: Option<String>,
    tests: Option<String>,
    files: Vec<String>,
    session_id: Option<String>,
) -> Result<()> {
    if done.is_none()
        && next.is_none()
        && blockers.is_none()
        && tests.is_none()
        && files.is_empty()
    {
        return Err(anyhow!(
            "at least one of --done, --next, --blockers, --tests, or --files is required"
        ));
    }

    let created_at_ms = current_time_ms()?;
    let files_json = serde_json::to_string(&files)?;

    conn.execute(
        "INSERT INTO checkpoints (
            repo_path, branch, commit_sha, session_id,
            done_text, next_text, blockers_text, tests_text, files_json, created_at_ms
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            &scope.repo_path,
            &scope.branch,
            &scope.commit_sha,
            session_id,
            done,
            next,
            blockers,
            tests,
            files_json,
            created_at_ms
        ],
    )?;

    Ok(())
}

fn latest_checkpoint_for_scope(
    conn: &Connection,
    repo_path: &str,
    branch: &str,
) -> Result<Option<Checkpoint>> {
    let mut stmt = conn.prepare(
        "SELECT id, repo_path, branch, commit_sha, session_id, done_text, next_text, blockers_text, tests_text, files_json, created_at_ms
         FROM checkpoints
         WHERE repo_path = ?1 AND branch = ?2
         ORDER BY created_at_ms DESC, id DESC
         LIMIT 1",
    )?;

    let mut rows = stmt.query(params![repo_path, branch])?;
    if let Some(row) = rows.next()? {
        Ok(Some(row_to_checkpoint(row)?))
    } else {
        Ok(None)
    }
}

fn list_checkpoints_for_scope(
    conn: &Connection,
    repo_path: &str,
    branch: &str,
    limit: u32,
) -> Result<Vec<Checkpoint>> {
    let mut stmt = conn.prepare(
        "SELECT id, repo_path, branch, commit_sha, session_id, done_text, next_text, blockers_text, tests_text, files_json, created_at_ms
         FROM checkpoints
         WHERE repo_path = ?1 AND branch = ?2
         ORDER BY created_at_ms DESC, id DESC
         LIMIT ?3",
    )?;

    let mut rows = stmt.query(params![repo_path, branch, limit])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(row_to_checkpoint(row)?);
    }
    Ok(out)
}

fn row_to_checkpoint(row: &rusqlite::Row<'_>) -> Result<Checkpoint> {
    let id: i64 = row.get(0)?;
    let files_json: String = row.get(9)?;
    let files: Vec<String> = serde_json::from_str(&files_json)
        .with_context(|| format!("invalid files_json for checkpoint id {}", id))?;

    Ok(Checkpoint {
        id,
        repo_path: row.get(1)?,
        branch: row.get(2)?,
        commit_sha: row.get(3)?,
        session_id: row.get(4)?,
        done_text: row.get(5)?,
        next_text: row.get(6)?,
        blockers_text: row.get(7)?,
        tests_text: row.get(8)?,
        files,
        created_at_ms: row.get(10)?,
    })
}

fn print_checkpoint(c: &Checkpoint) {
    println!("repo: {}", c.repo_path);
    println!("branch: {}", c.branch);
    println!("commit: {}", c.commit_sha);
    println!("at: {}", format_ts(c.created_at_ms));
    if let Some(session_id) = &c.session_id {
        println!("session: {}", session_id);
    }
    if let Some(done) = &c.done_text {
        println!("done: {}", done);
    }
    if let Some(next) = &c.next_text {
        println!("next: {}", next);
    }
    if let Some(blockers) = &c.blockers_text {
        println!("blockers: {}", blockers);
    }
    if let Some(tests) = &c.tests_text {
        println!("tests: {}", tests);
    }
    if !c.files.is_empty() {
        println!("files: {}", c.files.join(", "));
    }
}

fn print_checkpoint_compact(c: &Checkpoint) {
    let done = truncate_for_log(c.done_text.as_deref().unwrap_or("-"), 96);
    let next = truncate_for_log(c.next_text.as_deref().unwrap_or("-"), 96);
    println!(
        "#{} [{}] done={} | next={}",
        c.id,
        format_ts(c.created_at_ms),
        done,
        next
    );
}

fn format_ts(ms: i64) -> String {
    match DateTime::<Utc>::from_timestamp_millis(ms) {
        Some(ts) => ts.to_rfc3339(),
        None => ms.to_string(),
    }
}

fn current_time_ms() -> Result<i64> {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock before unix epoch")?;
    Ok(i64::try_from(dur.as_millis()).context("timestamp overflow")?)
}

fn current_dir_fallback() -> Result<String> {
    let cwd = std::env::current_dir().context("failed to read current dir")?;
    Ok(cwd.to_string_lossy().to_string())
}

fn detect_scope() -> Result<ContextScope> {
    let repo_from_git = git_repo_root();
    let branch_from_git = git_value(["rev-parse", "--abbrev-ref", "HEAD"]);
    let commit_from_git = git_value(["rev-parse", "HEAD"]);

    let used_repo_fallback = repo_from_git.is_none();
    let used_branch_fallback = branch_from_git.is_none();
    let used_commit_fallback = commit_from_git.is_none();

    let repo_path = repo_from_git.unwrap_or(current_dir_fallback()?);
    let branch = branch_from_git.unwrap_or_else(|| "unknown".to_string());
    let commit_sha = commit_from_git.unwrap_or_else(|| "unknown".to_string());

    Ok(ContextScope {
        repo_path,
        branch,
        commit_sha,
        used_repo_fallback,
        used_branch_fallback,
        used_commit_fallback,
    })
}

fn warn_scope_fallback(scope: &ContextScope) {
    let mut reasons = Vec::new();
    if scope.used_repo_fallback {
        reasons.push("repo_path from current directory");
    }
    if scope.used_branch_fallback {
        reasons.push("branch set to 'unknown'");
    }
    if scope.used_commit_fallback {
        reasons.push("commit set to 'unknown'");
    }
    if reasons.is_empty() {
        return;
    }

    eprintln!(
        "warning: using fallback git scope ({}) for repo='{}', branch='{}'",
        reasons.join(", "),
        scope.repo_path,
        scope.branch
    );
}

fn truncate_for_log(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }
    if max_chars <= 3 {
        return ".".repeat(max_chars);
    }

    let prefix: String = input.chars().take(max_chars - 3).collect();
    format!("{prefix}...")
}

fn git_repo_root() -> Option<String> {
    git_value(["rev-parse", "--show-toplevel"])
}

fn git_value<const N: usize>(args: [&str; N]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8(output.stdout).ok()?;
    let trimmed = s.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_db_path() -> PathBuf {
        let test_id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!(
            "switch-test-{}-{}-{}",
            std::process::id(),
            current_time_ms().expect("time"),
            test_id
        ));
        fs::create_dir_all(&base).expect("create temp dir");
        base.join("switch.db")
    }

    fn fixed_scope() -> ContextScope {
        ContextScope {
            repo_path: "/tmp/switch-test-repo".to_string(),
            branch: "feature/scope-tests".to_string(),
            commit_sha: "abc123".to_string(),
            used_repo_fallback: false,
            used_branch_fallback: false,
            used_commit_fallback: false,
        }
    }

    fn insert_checkpoint_raw(conn: &Connection, scope: &ContextScope, done: &str, created_at_ms: i64) -> i64 {
        conn.execute(
            "INSERT INTO checkpoints (
                repo_path, branch, commit_sha, done_text, files_json, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                &scope.repo_path,
                &scope.branch,
                &scope.commit_sha,
                done,
                "[]",
                created_at_ms
            ],
        )
        .expect("insert raw checkpoint");
        conn.last_insert_rowid()
    }

    #[test]
    fn init_creates_schema_and_is_idempotent() {
        let db_path = temp_db_path();
        let conn1 = open_db(&db_path).expect("open db first time");
        let order_index_exists: i64 = conn1
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_checkpoints_repo_branch_time_id'",
                [],
                |row| row.get(0),
            )
            .expect("query tie-break index");
        assert_eq!(order_index_exists, 1);
        let old_index_exists: i64 = conn1
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_checkpoints_repo_branch_time'",
                [],
                |row| row.get(0),
            )
            .expect("query old index");
        assert_eq!(old_index_exists, 0);

        conn1
            .execute(
                "INSERT INTO checkpoints (repo_path, branch, commit_sha, files_json, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params!["/tmp/a", "main", "abc", "[]", 1_i64],
            )
            .expect("insert after first init");
        conn1
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_checkpoints_repo_branch_time
                 ON checkpoints (repo_path, branch, created_at_ms DESC)",
                [],
            )
            .expect("seed old index");
        drop(conn1);

        let conn2 = open_db(&db_path).expect("open db second time");
        let count: i64 = conn2
            .query_row("SELECT COUNT(*) FROM checkpoints", [], |row| row.get(0))
            .expect("count rows");
        assert_eq!(count, 1);
        let old_index_exists_after_reopen: i64 = conn2
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_checkpoints_repo_branch_time'",
                [],
                |row| row.get(0),
            )
            .expect("query old index after reopen");
        assert_eq!(old_index_exists_after_reopen, 0);
    }

    #[test]
    fn save_and_resume_round_trip() {
        let db_path = temp_db_path();
        let conn = open_db(&db_path).expect("open db");
        let scope = fixed_scope();

        save_checkpoint(
            &conn,
            &scope,
            Some("implemented parser".to_string()),
            Some("add tests".to_string()),
            Some("none".to_string()),
            Some("not run".to_string()),
            vec!["src/main.rs".to_string()],
            Some("claude-session".to_string()),
        )
        .expect("save checkpoint");

        let latest = latest_checkpoint_for_scope(&conn, &scope.repo_path, &scope.branch)
            .expect("query latest")
            .expect("checkpoint exists");

        assert_eq!(latest.repo_path.as_str(), scope.repo_path.as_str());
        assert_eq!(latest.branch.as_str(), scope.branch.as_str());
        assert_eq!(latest.commit_sha.as_str(), scope.commit_sha.as_str());
        assert_eq!(latest.done_text.as_deref(), Some("implemented parser"));
        assert_eq!(latest.next_text.as_deref(), Some("add tests"));
        assert_eq!(latest.files, vec!["src/main.rs"]);
        assert_eq!(latest.session_id.as_deref(), Some("claude-session"));
    }

    #[test]
    fn log_returns_desc_order_and_limit() {
        let db_path = temp_db_path();
        let conn = open_db(&db_path).expect("open db");
        let mut scope = fixed_scope();

        scope.commit_sha = "first".to_string();
        save_checkpoint(
            &conn,
            &scope,
            Some("first".to_string()),
            Some("first-next".to_string()),
            None,
            None,
            vec![],
            None,
        )
        .expect("first save");

        scope.commit_sha = "second".to_string();
        save_checkpoint(
            &conn,
            &scope,
            Some("second".to_string()),
            Some("second-next".to_string()),
            None,
            None,
            vec![],
            None,
        )
        .expect("second save");

        let logs = list_checkpoints_for_scope(&conn, &scope.repo_path, &scope.branch, 1)
            .expect("list logs");
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].done_text.as_deref(), Some("second"));
    }

    #[test]
    fn same_timestamp_uses_id_tiebreak_for_latest_and_log() {
        let db_path = temp_db_path();
        let conn = open_db(&db_path).expect("open db");
        let scope = fixed_scope();
        let ts = 123_456_789_i64;

        let first_id = insert_checkpoint_raw(&conn, &scope, "first", ts);
        let second_id = insert_checkpoint_raw(&conn, &scope, "second", ts);
        assert!(second_id > first_id);

        let latest = latest_checkpoint_for_scope(&conn, &scope.repo_path, &scope.branch)
            .expect("query latest")
            .expect("checkpoint exists");
        assert_eq!(latest.id, second_id);
        assert_eq!(latest.done_text.as_deref(), Some("second"));

        let logs = list_checkpoints_for_scope(&conn, &scope.repo_path, &scope.branch, 2)
            .expect("list logs");
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].id, second_id);
        assert_eq!(logs[0].done_text.as_deref(), Some("second"));
        assert_eq!(logs[1].id, first_id);
        assert_eq!(logs[1].done_text.as_deref(), Some("first"));
    }

    #[test]
    fn save_requires_at_least_one_payload_field() {
        let db_path = temp_db_path();
        let conn = open_db(&db_path).expect("open db");
        let scope = fixed_scope();

        let err = save_checkpoint(&conn, &scope, None, None, None, None, vec![], None)
            .expect_err("save should fail");
        let msg = format!("{err:#}");
        assert!(msg.contains("at least one of --done, --next, --blockers, --tests, or --files is required"));
    }

    #[test]
    fn invalid_files_json_returns_error() {
        let db_path = temp_db_path();
        let conn = open_db(&db_path).expect("open db");
        let scope = fixed_scope();

        conn.execute(
            "INSERT INTO checkpoints (
                repo_path, branch, commit_sha, done_text, files_json, created_at_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                &scope.repo_path,
                &scope.branch,
                &scope.commit_sha,
                "bad row",
                "{not-valid-json",
                current_time_ms().expect("time")
            ],
        )
        .expect("insert malformed row");

        let err = latest_checkpoint_for_scope(&conn, &scope.repo_path, &scope.branch)
            .expect_err("expected parse error");
        let msg = format!("{err:#}");
        assert!(msg.contains("invalid files_json for checkpoint id"));
    }

    #[test]
    fn truncate_for_log_applies_ellipsis() {
        assert_eq!(truncate_for_log("short", 10), "short");
        assert_eq!(truncate_for_log("abcdefghijklmnopqrstuvwxyz", 8), "abcde...");
        assert_eq!(truncate_for_log("abcdef", 3), "...");
    }
}
