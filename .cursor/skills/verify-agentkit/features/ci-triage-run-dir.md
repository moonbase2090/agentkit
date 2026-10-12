# Rookrunner run-directory triage

## Sub-features

- Tools `rookrunner_list_failed` and `rookrunner_read_job_log`
- Session under `out/sessions/<workflow_run_id>/`
- Structured diagnosis files when enabled

## How to get to it (user POV)

Run ci-triage with `--run-dir fixtures/rookrunner-run`.

## Driving it with scripts/verify-agentkit.sh

Included in `drive` step `rookrunner-run`. Expect non-zero turns and a diagnosis mentioning the fixture failure.

## Gotchas

- Requires `fixtures/rookrunner-run/jobs.json` and logs present in the repo.
- Mock script is keyed to the run dir path string.
