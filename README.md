# context0

Save where you left off. Resume in any AI tool — context follows your git branch.

<img width="1470" height="802" alt="Screenshot 2026-02-25 at 2 12 27 AM" src="https://github.com/user-attachments/assets/cd17149f-3199-4b66-9f06-fc7f142a1138" />

## Why

AI coding sessions have no memory between tools. If you start in Claude Code, move to Cursor, and come back, you lose context. `context0` fixes that with a structured checkpoint scoped to your current `git repo + branch` — so context follows your work, not your tool.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/SyedSibtainRazvi/context0/main/install.sh | sh
```

Works on macOS and Linux. Downloads the right prebuilt binary for your platform.

**Windows:** download the `.zip` from [Releases](../../releases) and add the binary to your PATH.


## Commands

```bash
# Save a checkpoint for the current repo + branch
context0 save \
  --done "implemented OAuth flow" \
  --next "add refresh token logic" \
  --blockers "waiting on API key from infra" \
  --tests "cargo test auth::" \
  --files src/auth.rs --files src/middleware.rs \
  --session my-session

# Resume latest checkpoint (human-readable)
context0 resume

# Resume as JSON (for scripting or piping)
context0 resume --json

# Show recent checkpoints
context0 log --limit 20

# Delete all checkpoints for current repo + branch
context0 clear

# Override repo/branch detection
context0 --repo /path/to/repo --branch feature/x resume

# Generate shell completions
context0 completions bash >> ~/.bashrc
context0 completions zsh >> ~/.zshrc
context0 completions fish > ~/.config/fish/completions/context0.fish

# Start the MCP stdio server
context0 mcp-server

# Install agent rule files into the current project (one-time per project)
context0 init-rules
```

All commands auto-detect `repo`, `branch`, and `commit` from git. Use `--repo` and `--branch` to override.

## Docs

[https://syedsibtainrazvi.github.io/context0](https://syedsibtainrazvi.github.io/context0)

## MCP Server

`context0 mcp-server` starts a stdio MCP server that exposes three tools:

| Tool | Description |
|---|---|
| `get_context` | Get the latest checkpoint for a repo + branch |
| `save_context` | Save a new checkpoint |
| `list_context` | List recent checkpoints |

This lets Claude Code, Cursor, and Codex call `context0` natively — no manual terminal commands.

### Agent-driven workflow (recommended)

With MCP configured and rule files in place, the AI agent saves and resumes context for you automatically.

**Step 1 — install rule files** (run once per project):

```bash
cd your-project
context0 init-rules
```

This writes the right file to the right place for each tool:
- `CLAUDE.md` — Claude Code
- `.cursor/rules/context0.mdc` — Cursor
- `AGENTS.md` — Codex

Idempotent: safe to re-run, won't duplicate.

**Step 2 — configure MCP** for your tool (see Claude Code / Cursor below).

**That's it.** On session start the agent calls `get_context` and resumes. When you say "save context" or "I'm switching", the agent calls `save_context` with a full summary. No manual CLI needed.

### Claude Code

Add to `~/.claude/settings.json`:

```json
{
  "mcpServers": {
    "context0": {
      "command": "context0",
      "args": ["mcp-server"]
    }
  }
}
```

### Cursor

Add to `.cursor/mcp.json` (project) or `~/.cursor/mcp.json` (global):

```json
{
  "mcpServers": {
    "context0": {
      "command": "context0",
      "args": ["mcp-server"]
    }
  }
}
```

## How it works

- Context is scoped by `git repo root + branch` — running `context0 resume` on `feature/auth` always returns that branch's last checkpoint
- Checkpoints are stored in a local SQLite database at `~/.context0/context0.db`
- No cloud, no auth, no runtime dependencies

## Storage

| Setting | Default |
|---|---|
| Database | `~/.context0/context0.db` |
| Override | `context0 --db /path/to/custom.db <command>` |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT
