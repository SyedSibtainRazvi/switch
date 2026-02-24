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
            save_checkpoint(&conn, done, next, blockers, tests, files, session)?;
            println!("Checkpoint saved");
        }
        Commands::Resume { json } => {
            if let Some(checkpoint) = latest_checkpoint(&conn)? {
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
            let rows = list_checkpoints(&conn, limit)?;
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

    let repo_path = git_repo_root().unwrap_or(current_dir_fallback()?);
    let branch = git_value(["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_else(|| "unknown".to_string());
    let commit_sha = git_value(["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".to_string());
    let created_at_ms = current_time_ms()?;
    let files_json = serde_json::to_string(&files)?;

    conn.execute(
        "INSERT INTO checkpoints (
            repo_path, branch, commit_sha, session_id,
            done_text, next_text, blockers_text, tests_text, files_json, created_at_ms
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            repo_path,
            branch,
            commit_sha,
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

fn latest_checkpoint(conn: &Connection) -> Result<Option<Checkpoint>> {
    let repo_path = git_repo_root().unwrap_or(current_dir_fallback()?);
    let branch = git_value(["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_else(|| "unknown".to_string());
    let mut stmt = conn.prepare(
        "SELECT id, repo_path, branch, commit_sha, session_id, done_text, next_text, blockers_text, tests_text, files_json, created_at_ms
         FROM checkpoints
         WHERE repo_path = ?1 AND branch = ?2
         ORDER BY created_at_ms DESC
         LIMIT 1",
    )?;

    let mut rows = stmt.query(params![repo_path, branch])?;
    if let Some(row) = rows.next()? {
        Ok(Some(row_to_checkpoint(row)?))
    } else {
        Ok(None)
    }
}

fn list_checkpoints(conn: &Connection, limit: u32) -> Result<Vec<Checkpoint>> {
    let repo_path = git_repo_root().unwrap_or(current_dir_fallback()?);
    let branch = git_value(["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_else(|| "unknown".to_string());
    let mut stmt = conn.prepare(
        "SELECT id, repo_path, branch, commit_sha, session_id, done_text, next_text, blockers_text, tests_text, files_json, created_at_ms
         FROM checkpoints
         WHERE repo_path = ?1 AND branch = ?2
         ORDER BY created_at_ms DESC
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
    let files_json: String = row.get(9)?;
    let files: Vec<String> = serde_json::from_str(&files_json).unwrap_or_default();

    Ok(Checkpoint {
        id: row.get(0)?,
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
    let done = c.done_text.clone().unwrap_or_else(|| "-".to_string());
    let next = c.next_text.clone().unwrap_or_else(|| "-".to_string());
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
