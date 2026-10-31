#!/usr/bin/env bash
set -euo pipefail

root="$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"
cd "$root"

tag_message=""
release_title=""
release_body=""
release_body_file=""
artifacts=()

while [ "$#" -gt 0 ]; do
  case "$1" in
    --tag-message)
      tag_message="${2:-}"
      shift 2
      ;;
    --release-title)
      release_title="${2:-}"
      shift 2
      ;;
    --release-body)
      release_body="${2:-}"
      shift 2
      ;;
    --release-body-file)
      release_body_file="${2:-}"
      shift 2
      ;;
    --artifact)
      artifacts+=("${2:-}")
      shift 2
      ;;
    *)
      echo "usage: check-release-text.sh [--tag-message S] [--release-title S] [--release-body S]" \
        "[--release-body-file PATH] [--artifact PATH]..." >&2
      exit 2
      ;;
  esac
done

if [ -n "$release_body_file" ] && [ -f "$release_body_file" ]; then
  release_body="$(cat "$release_body_file")"
fi

export CHECK_ROOT="$root"
export CHECK_TAG_MESSAGE="$tag_message"
export CHECK_RELEASE_TITLE="$release_title"
export CHECK_RELEASE_BODY="$release_body"
CHECK_ARTIFACTS="$(printf '%s\n' "${artifacts[@]+"${artifacts[@]}"}")"
export CHECK_ARTIFACTS

python3 <<'PY'
from __future__ import annotations

import os
import re
import subprocess
import sys
from pathlib import Path

root = Path(os.environ["CHECK_ROOT"])
allowed_email = "322824348+mb2090@users.noreply.github.com"

generic_patterns: list[tuple[str, re.Pattern[str]]] = [
    ("desktop", re.compile(r"DESKTOP-[A-Z0-9]+", re.I)),
    ("local_hostname", re.compile(r"\.local\b", re.I)),
    ("macos_users", re.compile(r"/Users/")),
    ("linux_home", re.compile(r"/home/[a-z]")),
    ("windows_users", re.compile(r"C:\\Users", re.I)),
    (
        "private_key",
        re.compile(r"-----BEGIN [A-Z ]*PRIVATE KEY-----"),
    ),
    (
        "github_token",
        re.compile(r"(ghp|gho|ghs|ghu|github_pat)_[A-Za-z0-9_]{20,}"),
    ),
    (
        "private_ip",
        re.compile(
            r"\b(10\.[0-9]+\.[0-9]+\.[0-9]+|192\.168\.[0-9]+\.[0-9]+|"
            r"172\.(1[6-9]|2[0-9]|3[01])\.[0-9]+\.[0-9]+)\b"
        ),
    ),
]

email_re = re.compile(
    r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}",
    re.I,
)


def load_extra_patterns() -> list[re.Pattern[str]]:
    extra: list[re.Pattern[str]] = []
    denylist_path = Path.home() / ".config" / "moonbase" / "release-denylist.txt"
    if denylist_path.is_file():
        for line in denylist_path.read_text(encoding="utf-8", errors="replace").splitlines():
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            extra.append(re.compile(line, re.I))
    env = os.environ.get("RELEASE_DENYLIST", "")
    if env:
        for line in env.splitlines():
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            extra.append(re.compile(line, re.I))
    return extra


def line_hits(line: str, patterns: list[re.Pattern[str]]) -> bool:
    if email_re.search(line):
        for match in email_re.finditer(line):
            if match.group(0).lower() != allowed_email.lower():
                return True
    for pat in patterns:
        if pat.search(line):
            return True
    return False


def scan_text(label: str, text: str, patterns: list[re.Pattern[str]], hits: list[str]) -> None:
    for idx, line in enumerate(text.splitlines(), start=1):
        if line_hits(line, patterns):
            hits.append(f"{label}:{idx}")


def collect_text_files() -> list[Path]:
    paths: list[Path] = []
    for pattern in ("CHANGELOG", "CHANGELOG.*", "release-notes", "release-notes.*"):
        paths.extend(root.glob(pattern))
    for name in ("README.md", "RELEASING.md", "SECURITY.md"):
        p = root / name
        if p.is_file():
            paths.append(p)
    docs = root / "docs"
    if docs.is_dir():
        paths.extend(docs.rglob("*.md"))
    seen: set[Path] = set()
    ordered: list[Path] = []
    for p in paths:
        if p.is_file() and p not in seen:
            seen.add(p)
            ordered.append(p)
    return sorted(ordered)


def scan_file(path: Path, patterns: list[re.Pattern[str]], hits: list[str]) -> None:
    try:
        data = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return
    rel = path.relative_to(root).as_posix()
    scan_text(rel, data, patterns, hits)


def scan_strings(path: Path, patterns: list[re.Pattern[str]], hits: list[str]) -> None:
    try:
        proc = subprocess.run(
            ["strings", "-a", str(path)],
            check=False,
            capture_output=True,
            text=True,
        )
    except OSError:
        return
    if proc.returncode != 0 and not proc.stdout:
        return
    rel = path.relative_to(root).as_posix() if path.is_relative_to(root) else path.as_posix()
    for idx, line in enumerate(proc.stdout.splitlines(), start=1):
        if line_hits(line, patterns):
            hits.append(f"{rel}:strings:{idx}")
            return


def main() -> int:
    patterns = [p for _, p in generic_patterns] + load_extra_patterns()
    hits: list[str] = []

    for path in collect_text_files():
        scan_file(path, patterns, hits)

    tag_message = os.environ.get("CHECK_TAG_MESSAGE", "")
    release_title = os.environ.get("CHECK_RELEASE_TITLE", "")
    release_body = os.environ.get("CHECK_RELEASE_BODY", "")
    if tag_message:
        scan_text("tag-message", tag_message, patterns, hits)
    if release_title:
        scan_text("release-title", release_title, patterns, hits)
    if release_body:
        scan_text("release-body", release_body, patterns, hits)

    artifact_paths = os.environ.get("CHECK_ARTIFACTS", "").split("\n")
    for raw in artifact_paths:
        if not raw:
            continue
        path = Path(raw)
        if not path.is_file():
            continue
        if path.suffix in {".md", ".txt", ".json"}:
            scan_file(path, patterns, hits)
        else:
            scan_strings(path, patterns, hits)

    dist = root / "dist"
    if dist.is_dir():
        for path in sorted(dist.rglob("*")):
            if path.is_file():
                scan_strings(path, patterns, hits)

    if hits:
        for item in sorted(set(hits)):
            print(item)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
PY
