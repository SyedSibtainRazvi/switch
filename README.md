# switch

Local-first context handoff for AI coding agents.

Save a checkpoint when you stop. Resume it in any tool — Claude Code, Cursor, Codex — on the same repo and branch, with one command.

```
switch save --done "wired auth middleware" --next "fix integration tests"
switch resume
```

## Why

AI coding sessions have no memory between tools. If you start in Claude Code, move to Cursor, and come back, you lose context. `switch` fixes that with a structured checkpoint scoped to your current `git repo + branch` — so context follows your work, not your tool.

## Install

```bash
cargo install --path .
```

Or download a prebuilt binary from [Releases](../../releases).

## Commands

```bash
# Save a checkpoint for the current repo + branch
switch save \
  --done "implemented OAuth flow" \
  --next "add refresh token logic" \
  --blockers "waiting on API key from infra" \
  --tests "cargo test auth::" \
  --files src/auth.rs src/middleware.rs \
  --session my-session

# Resume latest checkpoint (human-readable)
switch resume

# Resume as JSON (for scripting or piping)
switch resume --json

# Show recent checkpoints
switch log --limit 20

# Delete all checkpoints for current repo + branch
switch clear

# Override repo/branch detection
switch --repo /path/to/repo --branch feature/x resume

# Generate shell completions
switch completions bash >> ~/.bashrc
switch completions zsh >> ~/.zshrc
switch completions fish > ~/.config/fish/completions/switch.fish

# Start the MCP stdio server
switch mcp-server
```

All commands auto-detect `repo`, `branch`, and `commit` from git. Use `--repo` and `--branch` to override.

## Web Docs

A clickable documentation page is available at:

- `webapp/index.html`

Open it directly in your browser, or run a local server:

```bash
cd webapp
python3 -m http.server 8080
```

Then visit `http://localhost:8080`.

## MCP Server

`switch mcp-server` starts a stdio MCP server that exposes three tools:

| Tool | Description |
|---|---|
| `get_context` | Get the latest checkpoint for a repo + branch |
| `save_context` | Save a new checkpoint |
| `list_context` | List recent checkpoints |

This lets Claude Code and Cursor call `switch` natively — no manual terminal commands.

### Claude Code

Add to `~/.claude/settings.json`:

```json
{
  "mcpServers": {
    "switch": {
      "command": "switch",
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
    "switch": {
      "command": "switch",
      "args": ["mcp-server"]
    }
  }
}
```

## How it works

- Context is scoped by `git repo root + branch` — running `switch resume` on `feature/auth` always returns that branch's last checkpoint
- Checkpoints are stored in a local SQLite database at `~/.switch/switch.db`
- No cloud, no auth, no runtime dependencies

## Storage

| Setting | Default |
|---|---|
| Database | `~/.switch/switch.db` |
| Override | `switch --db /path/to/custom.db <command>` |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT
