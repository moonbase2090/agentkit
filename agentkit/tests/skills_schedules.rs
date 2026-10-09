use std::path::PathBuf;

use agentkit::schedule::{is_due, ScheduleSpec, ScheduleState};
use agentkit::tools::{builtin_load_skill, register_ci_builtins};
use agentkit::{
    compose_system_prompt, load_agent, run_agent, MockProvider, RunOptions, SessionLog,
    ToolRegistry,
};
use chrono::{TimeZone, Utc};
use tempfile::tempdir;

#[test]
fn system_prompt_includes_skill_catalog() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../agents/ci-triage");
    let manifest = load_agent(&root).unwrap();
    let prompt = compose_system_prompt(&manifest);
    assert!(prompt.contains("## Skills"));
    assert!(prompt.contains("triage-hints"));
}

#[test]
fn load_skill_builtin_reads_body() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../agents/ci-triage");
    let manifest = load_agent(&root).unwrap();
    let ctx = agentkit::ToolContext {
        secrets: std::collections::HashMap::new(),
        command_timeout: std::time::Duration::from_secs(30),
        skills: manifest.skills.clone(),
        workspace_root: root.clone(),
        rookrunner_run_dir: None,
    };
    let body =
        builtin_load_skill(&serde_json::json!({ "skill_id": "triage-hints" }), &ctx).unwrap();
    assert!(body.contains("DIAGNOSIS"));
}

#[test]
fn schedule_due_when_cron_fires() {
    let spec = ScheduleSpec {
        id: "daily".to_owned(),
        cron: "0 0 9 * * *".to_owned(),
        prompt: "go".to_owned(),
    };
    let last = Utc.with_ymd_and_hms(2026, 10, 7, 9, 0, 0).unwrap();
    let now = Utc.with_ymd_and_hms(2026, 10, 8, 9, 30, 0).unwrap();
    assert!(is_due(&spec, Some(last), now).unwrap());
    let not_yet = Utc.with_ymd_and_hms(2026, 10, 8, 8, 0, 0).unwrap();
    assert!(!is_due(&spec, Some(last), not_yet).unwrap());
}

#[test]
fn agent_loads_schedules_from_toml() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../agents/ci-triage");
    let manifest = load_agent(&root).unwrap();
    assert!(manifest.schedules.iter().any(|s| s.id == "fixture-triage"));
}

#[test]
fn run_due_path_writes_session_when_forced_now() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    std::env::set_current_dir(&workspace).unwrap();
    let root = workspace.join("agents/ci-triage");
    let manifest = load_agent(&root).unwrap();
    let dir = tempdir().unwrap();
    let state_path = dir.path().join("state.json");
    let now = Utc.with_ymd_and_hms(2026, 10, 8, 10, 0, 0).unwrap();
    let spec = manifest
        .schedules
        .iter()
        .find(|s| s.id == "fixture-triage")
        .cloned()
        .unwrap();
    assert!(is_due(&spec, None, now).unwrap());

    let session_path = dir.path().join("session.jsonl");
    let session = SessionLog::open(&session_path).unwrap();
    let mut registry = ToolRegistry::from_manifest(&manifest);
    register_ci_builtins(&mut registry);
    let provider = MockProvider::scripted(agentkit::runtime::mock_ci_triage_script());
    run_agent(
        &manifest,
        &session,
        &provider,
        &registry,
        &spec.prompt,
        RunOptions::default(),
    )
    .unwrap();

    let mut state = ScheduleState::default();
    state.last_run.insert(spec.id.clone(), now);
    agentkit::save_state(&state_path, &state).unwrap();
    assert!(session.replay().unwrap().len() > 2);
}
