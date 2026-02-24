CREATE TABLE IF NOT EXISTS checkpoints (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_path TEXT NOT NULL,
    branch TEXT NOT NULL,
    commit_sha TEXT NOT NULL,
    session_id TEXT,
    done_text TEXT,
    next_text TEXT,
    blockers_text TEXT,
    tests_text TEXT,
    files_json TEXT NOT NULL DEFAULT '[]',
    created_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_checkpoints_repo_branch_time
ON checkpoints (repo_path, branch, created_at_ms DESC);

CREATE INDEX IF NOT EXISTS idx_checkpoints_repo_branch_time_id
ON checkpoints (repo_path, branch, created_at_ms DESC, id DESC);
