# context0 context handoff (Codex)

This project uses **context0** for context handoff between AI coding tools. You have access to the context0 MCP server; use its tools so the user can resume work in another tool (e.g. Cursor, Claude Code) without losing context.

## On session start

- Call **get_context** with the current repo root path and git branch (infer from project path and git: `git rev-parse --show-toplevel`, `git branch --show-current`).
- If a checkpoint is returned (`found: true`), read it carefully. It was written by a previous AI session. Use `done_text`, `next_text`, `blockers_text`, `tests_text`, and `files` to resume the task. Briefly confirm to the user what context you loaded and what you will do next.

## When the user ends the session or switches tools

When the user says "save context", "I'm switching", "save my session", or "I'm done for now":
- Call **save_context** with a structured summary of this session:
  - **done_text**: What was accomplished (concrete and specific).
  - **next_text**: What should happen next when work resumes.
  - **blockers_text**: Any blockers, waiting on, or open questions.
  - **tests_text**: Test status or commands to run (e.g. "cargo test", "npm test").
  - **files**: Key files that were created or changed (paths relative to repo root).
- Use the current repo path, branch, and commit SHA from git. Be concise; another AI agent will read this to resume.

## Optional

- **list_context** returns recent checkpoints for the same repo + branch if the user wants to see history or pick an older checkpoint.
