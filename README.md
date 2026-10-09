# agentkit

Rust library for filesystem-defined agents, JSONL (or optional SQLite) sessions, OpenAI-style tool calling, and a CI triage demo consumer.

## Agent layout

Each agent lives in a directory (for example `agents/ci-triage/`):

| Path | Purpose |
|------|---------|
| `instructions.md` | System instructions |
| `agent.toml` | Model id, `max_turns`, `command_timeout_secs` |
| `tools/*.json` | Tool manifests (`kind: builtin` or `command`) |
| `skills/*.md` | Skill bodies; catalog is injected into the system prompt |
| `schedules/*.toml` | Cron schedules (`id`, `cron`, `prompt`) |

Built-in tools are implemented in Rust and registered by id. Command tools spawn a process, send JSON args on stdin, and read stdout.

## Sessions

Append-only `session.jsonl` records typed events (`system_prompt`, `user_message`, tool calls/results, diagnosis). Resume with `--resume` on the demo binary; terminal sessions skip a new provider round.

Optional SQLite storage (same event schema):

```sh
cargo test -p agentkit --features sqlite-sessions
```

Use `SessionLog::open_sqlite(path)` from code when the feature is enabled.

## CI triage demo (mock provider)

```sh
cargo run -q -p demo --bin ci-triage
cargo run -q -p demo --bin ci-triage -- --run-dir fixtures/rookrunner-run
cargo run -q -p demo --bin ci-triage -- --resume
```

Rookrunner-style run directories contain `jobs.json` and `logs/<job>.log`. Per-run artifacts land under `out/sessions/<workflow_run_id>/`.

## Schedules (no daemon)

`run-due` checks cron expressions against `agents/<agent>/out/schedule-state.json` and runs due prompts once:

```sh
cargo run -q -p demo --bin run-due -- --now 2026-10-08T10:00:00Z
```

## License

MPL-2.0. See [LICENSE](LICENSE) and [NOTICE.txt](NOTICE.txt). To report a vulnerability, see [SECURITY.md](SECURITY.md).

## Development

```sh
cargo fmt
cargo clippy -- -D warnings
cargo test
```

Tests use the mock chat provider only. Live OpenRouter requires `OPENROUTER_API_KEY` and `--live` on the demo binary.
