use std::path::PathBuf;

use agentkit::runtime::mock_diagnosis_from_log;
use agentkit::tools::register_ci_builtins;
use agentkit::{
    allocate_session_path, load_agent, run_agent, Event, MockProvider, ProviderResponse,
    RunOptions, SessionLog, ToolCall, ToolRegistry,
};
use tempfile::tempdir;

#[test]
fn tool_failure_returns_error_result_and_continues() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    std::env::set_current_dir(&workspace).unwrap();
    let manifest = load_agent(&workspace.join("agents/ci-triage")).unwrap();
    let dir = tempdir().unwrap();
    let session = SessionLog::open(&dir.path().join("session.jsonl")).unwrap();
    let mut registry = ToolRegistry::from_manifest(&manifest);
    register_ci_builtins(&mut registry);

    let provider = MockProvider::scripted(vec![
        ProviderResponse {
            message: String::new(),
            tool_calls: vec![ToolCall {
                id: "1".to_owned(),
                name: "read_log".to_owned(),
                arguments: serde_json::json!({ "path": "missing.log" }),
            }],
        },
        ProviderResponse {
            message: "recovered after tool error".to_owned(),
            tool_calls: Vec::new(),
        },
    ]);

    let outcome = run_agent(
        &manifest,
        &session,
        &provider,
        &registry,
        "go",
        RunOptions {
            workspace_root: Some(workspace.clone()),
            rookrunner_run_dir: None,
            ..RunOptions::default()
        },
    )
    .unwrap();
    assert_eq!(outcome.turns, 2);
    let events = session.replay().unwrap();
    assert!(events.iter().any(|event| matches!(
        event,
        Event::ToolResult { content, .. } if content.starts_with("error:")
    )));
    assert!(events
        .iter()
        .any(|event| matches!(event, Event::ToolAudit { ok: false, .. })));
}

#[test]
fn run_dir_mode_exposes_only_rookrunner_tools() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let manifest = load_agent(&workspace.join("agents/ci-triage")).unwrap();
    let schemas = ToolRegistry::tool_schemas_rookrunner_run_dir(&manifest);
    assert_eq!(schemas.len(), 2);
    let offered: Vec<String> = schemas
        .iter()
        .filter_map(|schema| {
            schema
                .pointer("/function/name")
                .and_then(|value| value.as_str())
                .map(str::to_owned)
        })
        .collect();
    assert!(!offered.iter().any(|name| name == "list_failed_jobs"));
    assert!(offered.iter().any(|name| name == "rookrunner_list_failed"));
    let list_schema = schemas
        .iter()
        .find(|schema| {
            schema.pointer("/function/name").and_then(|v| v.as_str())
                == Some("rookrunner_list_failed")
        })
        .expect("list schema");
    assert!(list_schema
        .pointer("/function/parameters/properties/run_dir")
        .is_none());
}

#[test]
fn allocate_session_path_rotates_on_repeat_run() {
    let dir = tempdir().unwrap();
    let base = dir.path().join("session.jsonl");
    let session = SessionLog::open(&base).unwrap();
    session
        .append(&Event::UserMessage {
            content: "first".to_owned(),
        })
        .unwrap();

    let second = allocate_session_path(&base, false).unwrap();
    assert_ne!(second, base);
    assert!(second.to_string_lossy().contains("session-1"));
}

#[test]
fn mock_diagnosis_matches_fixture_log() {
    let log = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/ci.log"),
    )
    .unwrap();
    let diagnosis = mock_diagnosis_from_log(&log);
    assert!(diagnosis.contains("fill_wide_char_in_one_column_area_becomes_blank"));
    assert!(diagnosis.contains("assertion failed"));
}
