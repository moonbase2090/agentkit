# run-due schedules

## Sub-features

- Cron evaluation against `agents/ci-triage/schedules/`
- Writes per-schedule session jsonl under agent `out/`

## How to get to it (user POV)

`cargo run -q -p demo --bin run-due -- --now <RFC3339>` from repo root.

## Driving it with scripts/verify-agentkit.sh

Not part of default `drive` (schedules are optional in agent tree). Run manually after changing `schedules/` or `run_due.rs`.

## Gotchas

- Needs at least one schedule file and a frozen `--now` that matches cron in tests/docs.
