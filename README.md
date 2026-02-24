# switch

Local-first shared context for coding agents across the same repo/branch.

## What it does (v0)

- Save checkpoints: `done`, `next`, `blockers`, `tests`, touched files.
- Resume latest context for current `repo + branch`.
- Keep everything in a local SQLite database.

## Commands

```bash
switch init
switch save --done "wired auth middleware" --next "fix integration tests" --files src/auth.rs --tests "cargo test auth::"
switch resume
switch resume --json
switch log --limit 20
```

## Notes

- Database default path: `~/.switch/switch.db`
- Context scope key: `git repo root + git branch`
- This scaffold is local-only, no cloud sync.
