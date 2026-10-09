# Agentkit spike (Oct 2026)

## What was built

A small Cargo workspace (the original spike, since folded into this repository) with:

- **`agentkit`** — load agents from disk, JSONL session log + replay, OpenAI-style tool schemas, `ChatProvider` trait (`MockProvider`, `OpenRouterProvider` via ureq), tool dispatch (Rust builtins + JSON stdin/stdout command tools).
- **`demo` / `ci-triage`** — CI triage demo using `agents/ci-triage/` and fixtures under `fixtures/`.
- **Agent layout** — `instructions.md`, `agent.toml`, `tools/*.json`; `skills/*.md` and `schedules/` paths are discovered and stored on the manifest (not yet injected into prompts on demand).

## Scale

| Item | Count |
|------|------:|
| Rust source (`.rs`, excl. `target/`) | ~865 LOC |
| All project text (rs + toml + md + json, excl. `target/`) | ~939 LOC |

**Crates:** `anyhow`, `serde`/`serde_json`, `toml`, `walkdir`, `ureq` (+ TLS), `tempfile` (tests).

## Feature coverage (spike)

| Area | Spike status | Notes |
|------|--------------|-------|
| Filesystem agent dir | **Done** | Required files validated; per-tool JSON with `kind: builtin \| command`. |
| Typed tools + schemas | **Done** | OpenAI `tools` JSON built from manifests; secrets via env name on command tools only. |
| Command tool | **Done** | Spawn process, JSON args on stdin, stdout as result. |
| Skills on demand | **Partial** | Paths listed; no lazy load into context yet. |
| Schedules (cron) | **Stub** | Paths listed; no scheduler. |
| Durable sessions | **Done (JSONL)** | Append-only events; replay rebuilds chat messages; `--resume` skips new user turn. |
| Provider | **Done** | OpenRouter-compatible POST; mock for tests. |
| Channels / sandboxes / subagents | **Out of scope** | Would be separate binaries: HTTP ingress, `bubblewrap`/containers, child sessions with own JSONL. |

## Durability approach and limits

- **Approach:** one append-only `session.jsonl` per run; each line is a typed `Event` (`system_prompt`, `user_message`, `tool_call`, `tool_result`, `assistant_message`, `diagnosis`).
- **Resume:** `SessionLog::replay()` + `run_agent(..., resume_only: true)` does not append a duplicate user message; provider is invoked from reconstructed messages. Demo: run once, then `cargo run -p demo --bin ci-triage -- --resume` continues from the same log.
- **Limits:** no compaction or snapshot; concurrent writers unsupported; tool results are plain strings (no binary); replay re-calls the model on resume (idempotent only if provider is deterministic — mock is, live API is not); no encryption at rest; diagnosis detection is heuristic (`DIAGNOSIS:` / `Root cause:`).

## Kill/resume demo

```sh
rm -f out/session.jsonl out/diagnosis.txt
cargo run -p demo --bin ci-triage
# simulate kill: log already has events; re-run with --resume
cargo run -p demo --bin ci-triage -- --resume
cat out/diagnosis.txt
```

Live OpenRouter (optional, costs a few cents):

```sh
export OPENROUTER_API_KEY=...   # do not log
cargo run -p demo --bin ci-triage -- --live
```

## Tests (exact output, Oct 8 2026)

```sh
cargo test
```

```
running 3 tests
test loads_agent_manifest_and_validates_tools ... ok
test session_replay_survives_kill_and_resume ... ok
test mock_agent_runs_tools_and_writes_diagnosis ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Mock demo:

```
turns=3 diagnosis=Some("DIAGNOSIS: test job failed because the workspace test hit a missing import in grid.rs.")
```

## v0.1 estimate

| Consumer | Scope | Effort (rough) |
|----------|--------|----------------|
| **Rookrunner CI triage** | Real log/job fetch tools, GitHub API auth via env, structured diagnosis JSON, session per workflow run, integration test with recorded fixtures | **2–3 weeks** |
| **Scorecard finding re-check** | Skill loading, one “re-run check” tool wrapping existing Scorecard CLI, tighter turn limits, audit log | **1–2 weeks** after shared v0.1 core |

Shared v0.1 core (error taxonomy, skill injection, SQLite option for large logs, command-tool timeouts): **~3–4 weeks** one engineer.

## Risks

- Live model non-determinism on resume unless we skip re-completion when the log already ends in a terminal assistant message.
- OpenRouter / model JSON quirks for tool calls.
- Command tools need sandboxing before production (this spike runs them as the host user).
- No observability (tracing, cost accounting) yet.

## Recommendation

**Go**, with a slightly narrower v0.1: ship **filesystem agents + JSONL sessions + OpenRouter + builtins/command tools** for Rookrunner first; add **skills-on-demand** and **schedules** in v0.1.1. The Rust shape maps cleanly; the hard parts are ops (sandbox, secrets, channel ingress), not the agent loop itself.
