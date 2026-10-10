# Releasing

## Public release content

Release text and artifacts must not reference private infrastructure, hostnames, personal paths, or internal tooling.

This applies to release titles and bodies, tag messages, changelog and release-note files, README and other published docs, and strings embedded in shipped binaries or archives.

## Automated check

Before a release is published, `scripts/check-release-text.sh` scans the paths above plus built artifacts. It fails on forbidden patterns (generic rules in the script) and on optional extra rules from a local denylist file or the `RELEASE_DENYLIST` secret. CI prints only `path:line` locations, not matched text.

Run locally after building:

```sh
cargo build --release -p demo
scripts/check-release-text.sh \
  --tag-message "$(git tag -l --format='%(contents)' v0.1.1 2>/dev/null || true)" \
  --release-title "v0.1.1" \
  --release-body-file CHANGELOG.md \
  --artifact target/release/ci-triage \
  --artifact target/release/run-due
```

Missing changelog or artifact paths are skipped when not present.
