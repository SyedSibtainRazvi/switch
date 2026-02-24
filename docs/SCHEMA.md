# SCHEMA

## Table: `checkpoints`

- `id INTEGER PRIMARY KEY AUTOINCREMENT`
- `repo_path TEXT NOT NULL`
- `branch TEXT NOT NULL`
- `commit_sha TEXT NOT NULL`
- `session_id TEXT NULL`
- `done_text TEXT NULL`
- `next_text TEXT NULL`
- `blockers_text TEXT NULL`
- `tests_text TEXT NULL`
- `files_json TEXT NOT NULL DEFAULT '[]'`
- `created_at_ms INTEGER NOT NULL`

## Index

- `idx_checkpoints_repo_branch_time_id (repo_path, branch, created_at_ms DESC, id DESC)`

## Storage key

`repo_path + branch` defines context scope for resume/log queries.
