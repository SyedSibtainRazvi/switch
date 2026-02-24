# SPEC

## Goal

`switch` provides deterministic context handoff between coding tools by storing per-repo/per-branch checkpoints locally.

## Scope (v0)

- Local CLI only.
- SQLite storage.
- Commands: `init`, `save`, `resume`, `log`.
- Auto-capture repo root, branch, commit from `git`.

## Non-goals (v0)

- Cloud sync.
- Vector search.
- Team auth/collaboration.
- Full chat transcript storage.

## Functional requirements

- `save` writes one checkpoint tied to current `repo + branch`.
- `resume` returns latest checkpoint for current `repo + branch`.
- `log` returns recent checkpoints in reverse chronological order.
- Each checkpoint stores evidence fields (`commit`, `files`, `tests`).

## Success criteria

- Switching tools preserves active task context with one command.
- Resume output is concise and deterministic.
