# Changelog

## [Unreleased]

## [0.1.2] - 2026-02-25

- Fix `--db` help text to show correct default path
- Bump version to match release tag

## [0.1.0] - 2026-02-25

- Initial release
- CLI commands: `init`, `save`, `resume`, `log`, `clear`, `init-rules`, `completions`
- MCP stdio server with `get_context`, `save_context`, `list_context` tools
- `init-rules` command — installs agent rule files for Claude Code, Cursor, and Codex in one step
- Agent rule files bundled in binary via `include_str!` (no repo clone needed)
- SQLite storage with WAL mode
- Auto-detection of git repo, branch, and commit
- `--repo` and `--branch` override flags
