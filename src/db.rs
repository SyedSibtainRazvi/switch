use anyhow::{anyhow, Context, Result};
use rusqlite::{params, Connection};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::checkpoint::{Checkpoint, CheckpointPayload};
use crate::git::ContextScope;

pub fn open_db(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create db dir {}", parent.display()))?;
    }

    let conn = Connection::open(path)
        .with_context(|| format!("failed to open sqlite db at {}", path.display()))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "busy_timeout", 5000)?;
    conn.execute_batch(include_str!("../migrations/0001_init.sql"))?;
    Ok(conn)
}

pub fn save_checkpoint(
    conn: &Connection,
    scope: &ContextScope,
    payload: &CheckpointPayload,
) -> Result<i64> {
    if payload.is_empty() {
        return Err(anyhow!(
            "at least one of --done, --next, --blockers, --tests, or --files is required"
        ));
    }

    let created_at_ms = current_time_ms()?;
    let files_json = serde_json::to_string(&payload.files)?;

    conn.execute(
        "INSERT INTO checkpoints (
            repo_path, branch, commit_sha, session_id,
            done_text, next_text, blockers_text, tests_text, files_json, created_at_ms
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            &scope.repo_path,
            &scope.branch,
            &scope.commit_sha,
            payload.session_id,
            payload.done,
            payload.next,
            payload.blockers,
            payload.tests,
            files_json,
            created_at_ms
        ],
    )?;

    Ok(conn.last_insert_rowid())
}

pub fn latest_checkpoint_for_scope(
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

pub fn list_checkpoints_for_scope(
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

pub fn delete_checkpoints_for_scope(
    conn: &Connection,
    repo_path: &str,
    branch: &str,
) -> Result<usize> {
    let count = conn.execute(
        "DELETE FROM checkpoints WHERE repo_path = ?1 AND branch = ?2",
        params![repo_path, branch],
    )?;
    Ok(count)
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

pub fn current_time_ms() -> Result<i64> {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock before unix epoch")?;
    i64::try_from(dur.as_millis()).context("timestamp overflow")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_db_path() -> PathBuf {
        let test_id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!(
            "context0-test-{}-{}-{}",
            std::process::id(),
            current_time_ms().expect("time"),
            test_id
        ));
        fs::create_dir_all(&base).expect("create temp dir");
        base.join("context0.db")
    }

    fn fixed_scope() -> ContextScope {
        ContextScope {
            repo_path: "/tmp/context0-test-repo".to_string(),
            branch: "feature/scope-tests".to_string(),
            commit_sha: "abc123".to_string(),
            used_repo_fallback: false,
            used_branch_fallback: false,
            used_commit_fallback: false,
        }
    }

    fn insert_checkpoint_raw(
        conn: &Connection,
        scope: &ContextScope,
        done: &str,
        created_at_ms: i64,
    ) -> i64 {
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

        let payload = CheckpointPayload {
            done: Some("implemented parser".to_string()),
            next: Some("add tests".to_string()),
            blockers: Some("none".to_string()),
            tests: Some("not run".to_string()),
            files: vec!["src/main.rs".to_string()],
            session_id: Some("claude-session".to_string()),
        };
        save_checkpoint(&conn, &scope, &payload).expect("save checkpoint");

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
        let scope = fixed_scope();

        let payload1 = CheckpointPayload {
            done: Some("first".to_string()),
            next: Some("first-next".to_string()),
            blockers: None,
            tests: None,
            files: vec![],
            session_id: None,
        };
        save_checkpoint(&conn, &scope, &payload1).expect("first save");

        let payload2 = CheckpointPayload {
            done: Some("second".to_string()),
            next: Some("second-next".to_string()),
            blockers: None,
            tests: None,
            files: vec![],
            session_id: None,
        };
        save_checkpoint(&conn, &scope, &payload2).expect("second save");

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

        let payload = CheckpointPayload {
            done: None,
            next: None,
            blockers: None,
            tests: None,
            files: vec![],
            session_id: None,
        };
        let err = save_checkpoint(&conn, &scope, &payload).expect_err("save should fail");
        let msg = format!("{err:#}");
        assert!(msg.contains(
            "at least one of --done, --next, --blockers, --tests, or --files is required"
        ));
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
    fn delete_removes_checkpoints_for_scope() {
        let db_path = temp_db_path();
        let conn = open_db(&db_path).expect("open db");
        let scope = fixed_scope();

        let payload = CheckpointPayload {
            done: Some("to be deleted".to_string()),
            next: None,
            blockers: None,
            tests: None,
            files: vec![],
            session_id: None,
        };
        save_checkpoint(&conn, &scope, &payload).expect("save checkpoint");

        let count = delete_checkpoints_for_scope(&conn, &scope.repo_path, &scope.branch)
            .expect("delete checkpoints");
        assert_eq!(count, 1);

        let latest = latest_checkpoint_for_scope(&conn, &scope.repo_path, &scope.branch)
            .expect("query latest");
        assert!(latest.is_none());
    }
}
