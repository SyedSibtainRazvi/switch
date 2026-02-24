# PERF

## Targets (v0)

- `switch save`: p95 < 100ms (local SSD, warm cache)
- `switch resume`: p95 < 200ms
- `switch log --limit 20`: p95 < 200ms

## Method

- Use local benchmark script with fixed dataset sizes.
- Run with SQLite WAL mode enabled.
- Measure cold vs warm runs.
- Report p50/p95/p99 and max latency.

## Notes

- Expected bottlenecks are disk I/O and process startup, not CPU.
- Keep payload sizes small; avoid raw transcript storage in v0.
