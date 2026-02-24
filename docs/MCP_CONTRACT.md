# MCP_CONTRACT

Planned minimal MCP tools for editor/agent integration:

## `get_context`

Input:

```json
{
  "repo_path": "/abs/path/to/repo",
  "branch": "feature/x"
}
```

Output:

```json
{
  "found": true,
  "checkpoint": {
    "done_text": "...",
    "next_text": "...",
    "blockers_text": "...",
    "tests_text": "...",
    "files": ["src/main.rs"],
    "commit_sha": "abc123",
    "created_at_ms": 0
  }
}
```

## `save_context`

Input:

```json
{
  "repo_path": "/abs/path/to/repo",
  "branch": "feature/x",
  "session_id": "claude-1",
  "done_text": "...",
  "next_text": "...",
  "blockers_text": "...",
  "tests_text": "...",
  "files": ["src/main.rs"],
  "commit_sha": "abc123"
}
```

Output:

```json
{
  "ok": true,
  "id": 123
}
```

## `list_context`

Input:

```json
{
  "repo_path": "/abs/path/to/repo",
  "branch": "feature/x",
  "limit": 20
}
```

Output:

```json
{
  "items": []
}
```
