# Mock CI triage (fixture log)

## Sub-features

- Load `agents/ci-triage` manifest and builtins
- Mock provider runs scripted tool calls against `fixtures/ci.log`
- Writes session under proof `out/` and prints diagnosis

## How to get to it (user POV)

From repo root, run the ci-triage demo without flags (default fixture path).

## Driving it with scripts/verify-agentkit.sh

Included in `drive` step `fixture-triage`. Expect stdout matching `turns=` and `diagnosis=Some("DIAGNOSIS:`.

## Gotchas

- Writes `out/session.jsonl` (or rotated `session-N.jsonl`); the helper copies artifacts into `out/verify-proof/` after the run.
- Do not pass `--live` for verification.
