---
name: verify-agentkit
description: Drive agentkit via the demo CLIs (ci-triage, run-due) with the mock provider, capture session and diagnosis artifacts, and prove the agent loop works. Use before shipping PRs that change runtime, tools, sessions, or demos.
---

# Verify agentkit

agentkit is a Rust library; users and agents interact through the **demo binaries** and filesystem agent definitions under `agents/`. Verification never calls live OpenRouter unless you explicitly opt into `--live` (do not use that in routine proof).

## Launch

From the repository root:

```sh
cargo build -q -p demo
```

Ready when `target/debug/ci-triage` and `target/debug/run-due` exist. No long-lived server.

Each drive uses an isolated artifact directory (default `out/verify-proof/`). Teardown removes only that directory when you pass `--clean` to the helper (see Helpers).

## Doctor

```sh
scripts/verify-agentkit.sh doctor
```

Checks: repo root (has `agents/ci-triage/agent.toml`), demo binaries build, and `cargo test -q -p agentkit` passes. Exit non-zero stops the pass.

## Drive

Use the helper for a full mock pass:

```sh
scripts/verify-agentkit.sh drive
```

Manual equivalents:

1. **Fixture triage** — `cargo run -q -p demo --bin ci-triage` with `AGENTKIT_VERIFY_DIR` set so sessions land under the proof dir (helper sets this).
2. **Rookrunner run dir** — `cargo run -q -p demo --bin ci-triage -- --run-dir fixtures/rookrunner-run`
3. **Resume** — run triage once, then `cargo run -q -p demo --bin ci-triage -- --resume` against the same session path.

Feature-level steps live under `features/`.

## Evidence

After `drive`, the helper writes:

| Artifact | Meaning |
|----------|---------|
| `out/verify-proof/summary.txt` | Exit codes and one-line outcomes per step |
| `out/verify-proof/ci-triage.stdout` | Last ci-triage stdout (contains `turns=` and `diagnosis=`) |
| `out/verify-proof/session.jsonl` or `.../sessions/.../session-*.jsonl` | Append-only session log |
| `out/verify-proof/diagnosis.txt` | Diagnosis line when emitted |

Proof standards: use the **mock provider** only; assert stdout mentions a diagnosis, session file exists and contains `tool_call` / `tool_result` events, and rookrunner mode reads fixture logs (no network). Do not use `--live` for routine verification.

## Cleanup

```sh
scripts/verify-agentkit.sh clean
```

Removes `out/verify-proof/` only. Evidence for a completed proof should be copied elsewhere before clean if you need to attach it to a PR; the helper does not delete proof until you run `clean`.

## Helpers

```sh
scripts/verify-agentkit.sh doctor   # read-only health
scripts/verify-agentkit.sh drive    # build, run three mock flows, write summary
scripts/verify-agentkit.sh clean    # remove out/verify-proof
```

Optional: `AGENTKIT_VERIFY_DIR=/path` overrides the proof directory.
