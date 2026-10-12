# Session resume

## Sub-features

- JSONL session replay
- `--resume` skips duplicate user turn when log already has events

## How to get to it (user POV)

Run ci-triage once, then again with `--resume` on the same session file.

## Driving it with scripts/verify-agentkit.sh

Included in `drive` after fixture triage creates a session; second invocation uses `--resume`.

## Gotchas

- `fresh_run` rejects a non-empty session without `--resume`; the helper orders runs correctly.
