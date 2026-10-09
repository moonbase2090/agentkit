use std::path::PathBuf;

use agentkit::error::ErrorKind;
use agentkit::tools::register_ci_builtins;
use agentkit::{
    load_agent, run_agent,
    runtime::{mock_ci_triage_script, mock_rookrunner_triage_script},
    AgentError, Event, MockProvider, RunOptions, SessionLog, ToolRegistry,
};
use tempfile::tempdir;

#[test]
fn loads_agent_manifest_and_validates_tools() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../agents/ci-triage");
    let manifest = load_agent(&root).expect("load ci-triage agent");
    assert!(manifest.instructions.contains("CI triage"));
    assert_eq!(manifest.tools.len(), 5);
}

#[test]
fn session_replay_survives_kill_and_resume() {
    let dir = tempdir().unwrap();
    let log_path = dir.path().join("session.jsonl");
    let session = SessionLog::open(&log_path).unwrap();
    session
        .append(&Event::SystemPrompt {
            content: "system".to_owned(),
        })
        .unwrap();
    session
        .append(&Event::UserMessage {
            content: "go".to_owned(),
        })
        .unwrap();
    session
        .append(&Event::ToolCall {
            id: "1".to_owned(),
            name: "read_log".to_owned(),
            arguments: serde_json::json!({ "path": "fixtures/ci.log" }),
        })
        .unwrap();

    let replayed = SessionLog::open(&log_path).unwrap().replay().unwrap();
    assert_eq!(replayed.len(), 3);
    let messages = SessionLog::messages_from_events(&replayed);
    assert_eq!(messages.len(), 3);
}

#[test]
fn mock_agent_runs_tools_and_writes_diagnosis() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    std::env::set_current_dir(&workspace_root).unwrap();
    let root = workspace_root.join("agents/ci-triage");
    let manifest = load_agent(&root).unwrap();
    let dir = tempdir().unwrap();
    let log_path = dir.path().join("session.jsonl");
    let session = SessionLog::open(&log_path).unwrap();

    let mut registry = ToolRegistry::from_manifest(&manifest);
    register_ci_builtins(&mut registry);

    let provider = MockProvider::scripted(mock_ci_triage_script());
    let outcome = run_agent(
        &manifest,
        &session,
        &provider,
        &registry,
        "Triage the fixture failure.",
        RunOptions::default(),
    )
    .unwrap();
    assert!(outcome.diagnosis.is_some());
    assert!(outcome.structured.is_some());
    let events = session.replay().unwrap();
    assert!(events
        .iter()
        .any(|event| matches!(event, Event::ToolResult { .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event, Event::ToolAudit { .. })));
}

#[test]
fn rookrunner_run_dir_integration() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    std::env::set_current_dir(&workspace_root).unwrap();
    let run_dir = workspace_root.join("fixtures/rookrunner-run");
    let root = workspace_root.join("agents/ci-triage");
    let manifest = load_agent(&root).unwrap();
    let dir = tempdir().unwrap();
    let log_path = dir.path().join("session.jsonl");
    let session = SessionLog::open(&log_path).unwrap();

    let mut registry = ToolRegistry::from_manifest(&manifest);
    register_ci_builtins(&mut registry);

    let provider = MockProvider::scripted(mock_rookrunner_triage_script(
        run_dir.to_string_lossy().as_ref(),
    ));
    let outcome = run_agent(
        &manifest,
        &session,
        &provider,
        &registry,
        "Triage the rookrunner fixture run.",
        RunOptions {
            resume_only: false,
            failed_jobs_hint: vec!["test".to_owned()],
            rookrunner_run_dir: Some(run_dir.clone()),
            ..RunOptions::default()
        },
    )
    .unwrap();
    let structured = outcome.structured.expect("structured diagnosis");
    assert!(structured.summary.contains("DIAGNOSIS:"));
    assert_eq!(structured.failed_jobs, vec!["test".to_owned()]);
    assert!(structured.log_excerpt.is_some());
}

#[test]
fn resume_skips_provider_when_terminal() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    std::env::set_current_dir(&workspace_root).unwrap();
    let root = workspace_root.join("agents/ci-triage");
    let manifest = load_agent(&root).unwrap();
    let dir = tempdir().unwrap();
    let log_path = dir.path().join("session.jsonl");
    let session = SessionLog::open(&log_path).unwrap();

    let mut registry = ToolRegistry::from_manifest(&manifest);
    register_ci_builtins(&mut registry);

    let provider = MockProvider::scripted(mock_ci_triage_script());
    run_agent(
        &manifest,
        &session,
        &provider,
        &registry,
        "Triage the fixture failure.",
        RunOptions::default(),
    )
    .unwrap();

    let resume_provider = MockProvider::scripted(vec![]);
    let outcome = run_agent(
        &manifest,
        &session,
        &resume_provider,
        &registry,
        "ignored",
        RunOptions {
            resume_only: true,
            failed_jobs_hint: Vec::new(),
            ..RunOptions::default()
        },
    )
    .unwrap();
    assert_eq!(outcome.turns, 0);
    assert!(outcome.diagnosis.is_some());
}

#[test]
fn turn_limit_returns_taxonomy_error() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let root = workspace_root.join("agents/ci-triage");
    let manifest = load_agent(&root).unwrap();
    let mut config = manifest.config.clone();
    config.max_turns = 1;
    let manifest = agentkit::AgentManifest { config, ..manifest };

    let dir = tempdir().unwrap();
    let session = SessionLog::open(&dir.path().join("session.jsonl")).unwrap();
    let mut registry = ToolRegistry::from_manifest(&manifest);
    register_ci_builtins(&mut registry);

    let looping = vec![
        agentkit::ProviderResponse {
            message: String::new(),
            tool_calls: vec![agentkit::ToolCall {
                id: "1".to_owned(),
                name: "read_log".to_owned(),
                arguments: serde_json::json!({ "path": "fixtures/ci.log" }),
            }],
        },
        agentkit::ProviderResponse {
            message: String::new(),
            tool_calls: vec![agentkit::ToolCall {
                id: "2".to_owned(),
                name: "read_log".to_owned(),
                arguments: serde_json::json!({ "path": "fixtures/ci.log" }),
            }],
        },
    ];
    let provider = MockProvider::scripted(looping);
    std::env::set_current_dir(&workspace_root).unwrap();
    let err = run_agent(
        &manifest,
        &session,
        &provider,
        &registry,
        "loop",
        RunOptions::default(),
    )
    .unwrap_err();
    assert_eq!(err.kind, ErrorKind::TurnLimit);
}

#[test]
fn error_taxonomy_serializes() {
    let err = AgentError::tool("missing path");
    let json = serde_json::to_string(&err).unwrap();
    assert!(json.contains("tool"));
}
