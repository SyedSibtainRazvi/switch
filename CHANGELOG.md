# Changelog

## [Unreleased]

## [0.1.3] - 2026-02-26

- Fix SQLite busy timeout — use `conn.busy_timeout()` with 30s default, configurable via `CONTEXT0_BUSY_TIMEOUT_MS`
- Fix `--repo` override not controlling commit SHA detection — commit now resolved via `git -C <repo>` when `--repo` is provided
- Fix `--files` docs showing invalid space-separated syntax — correct form is repeated flags (`--files a --files b`)
- Landing page: add Windows install instructions, Codex MCP tab, shell completions, `--repo`/`--branch` overrides, split quickstart into MCP and CLI-only paths

## [0.1.2] - 2026-02-25

- Fix `--db` help text to show correct default path (`~/.context0/context0.db`)
- Fix release workflow to use `macos-latest` for Intel macOS cross-compile (retired `macos-13` runner)
- Responsive mobile layout for webapp — fix code block overflow, copy button position, font scaling, panel shadows
- Deploy webapp via GitHub Pages
- `curl | sh` install script — auto-detects platform, no Rust required

## [0.1.1] - 2026-02-25

- Rename project from `switch` to `context0`
- New tagline: git-scoped session state for AI coding agents
- `init-rules` now writes `.cursor/rules/context0.mdc`, `CLAUDE.md`, and `AGENTS.md`
- Updated MCP tool descriptions to coach agents on when and how to save context
- Agent rule files for Claude Code, Cursor, and Codex

## [0.1.0] - 2026-02-25

- Initial release
- CLI commands: `init`, `save`, `resume`, `log`, `clear`, `init-rules`, `completions`
- MCP stdio server with `get_context`, `save_context`, `list_context` tools
- `init-rules` command — installs agent rule files for Claude Code, Cursor, and Codex in one step
- Agent rule files bundled in binary via `include_str!` (no repo clone needed)
- SQLite storage with WAL mode
- Auto-detection of git repo, branch, and commit
- `--repo` and `--branch` override flags
