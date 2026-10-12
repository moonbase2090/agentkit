#!/usr/bin/env bash
set -euo pipefail

root="$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"
cd "$root"

proof_dir="${AGENTKIT_VERIFY_DIR:-$root/out/verify-proof}"

doctor() {
  test -f "$root/agents/ci-triage/agent.toml"
  cargo build -q -p demo
  cargo test -q -p agentkit
}

clean() {
  rm -rf "$proof_dir"
}

reset_out() {
  rm -f "$root/out"/session*.jsonl "$root/out"/diagnosis.txt "$root/out"/diagnosis.json
  rm -rf "$root/out/sessions"
  mkdir -p "$root/out"
}

archive_proof() {
  mkdir -p "$proof_dir"
  reset_out
  cp -a "$root/out"/session*.jsonl "$proof_dir/" 2>/dev/null || true
  cp -a "$root/out"/diagnosis.txt "$root/out"/diagnosis.json "$proof_dir/" 2>/dev/null || true
  cp -a "$root/out/sessions" "$proof_dir/" 2>/dev/null || true
}

drive() {
  doctor
  mkdir -p "$proof_dir"
  summary="$proof_dir/summary.txt"
  : >"$summary"

  reset_out
  stdout="$proof_dir/ci-triage.stdout"
  if cargo run -q -p demo --bin ci-triage >"$stdout" 2>&1; then
    echo "fixture-triage: ok" >>"$summary"
  else
    echo "fixture-triage: fail" >>"$summary"
    cat "$stdout" >&2
    exit 1
  fi
  grep -q 'diagnosis=Some("DIAGNOSIS:' "$stdout" || {
    echo "fixture-triage: missing diagnosis" >>"$summary"
    exit 1
  }
  test -f "$root/out/session.jsonl" || test -n "$(ls "$root/out"/session-*.jsonl 2>/dev/null)" || {
    echo "fixture-triage: no session log" >>"$summary"
    exit 1
  }
  cp "$stdout" "$proof_dir/ci-triage-first.stdout"

  if cargo run -q -p demo --bin ci-triage -- --resume >>"$stdout" 2>&1; then
    echo "fixture-resume: ok" >>"$summary"
  else
    echo "fixture-resume: fail" >>"$summary"
    exit 1
  fi

  rook_stdout="$proof_dir/ci-triage-run-dir.stdout"
  if cargo run -q -p demo --bin ci-triage -- --run-dir fixtures/rookrunner-run >"$rook_stdout" 2>&1; then
    echo "rookrunner-run: ok" >>"$summary"
  else
    echo "rookrunner-run: fail" >>"$summary"
    cat "$rook_stdout" >&2
    exit 1
  fi
  grep -q 'diagnosis=Some("DIAGNOSIS:' "$rook_stdout" || {
    echo "rookrunner-run: missing diagnosis" >>"$summary"
    exit 1
  }

  cp -a "$root/out"/session*.jsonl "$proof_dir/" 2>/dev/null || true
  cp -a "$root/out"/diagnosis.txt "$root/out"/diagnosis.json "$proof_dir/" 2>/dev/null || true
  cp -a "$root/out/sessions" "$proof_dir/" 2>/dev/null || true
  echo "drive: complete" >>"$summary"
}

case "${1:-}" in
  doctor) doctor ;;
  drive) drive ;;
  clean) clean ;;
  *)
    echo "usage: verify-agentkit.sh doctor|drive|clean" >&2
    exit 2
    ;;
esac
