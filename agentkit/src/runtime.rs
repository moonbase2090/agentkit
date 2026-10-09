use std::time::Instant;

use crate::agent::{compose_system_prompt, AgentManifest};
use crate::diagnosis::{enrich_diagnosis, parse_diagnosis_line};
use crate::error::{AgentError, AgentResult, ErrorKind};
use crate::provider::{ChatProvider, ProviderResponse};
use crate::session::{Event, SessionLog};
use crate::tools::{ToolContext, ToolRegistry};

#[derive(Debug)]
pub struct RunOutcome {
    pub diagnosis: Option<String>,
    pub structured: Option<crate::diagnosis::DiagnosisRecord>,
    pub turns: u32,
}

#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    pub resume_only: bool,
    pub failed_jobs_hint: Vec<String>,
    pub fresh_run: bool,
    pub exposed_tools: Option<Vec<String>>,
    pub workspace_root: Option<std::path::PathBuf>,
    pub rookrunner_run_dir: Option<std::path::PathBuf>,
}

pub fn mock_diagnosis_from_log(log: &str) -> String {
    let failed_test = log
        .lines()
        .find(|line| line.contains("... FAILED"))
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("test");
    let detail = log
        .lines()
        .find(|line| line.contains("assertion failed") || line.contains("panicked at"))
        .map(str::trim)
        .unwrap_or("see job log");
    format!("DIAGNOSIS: test job failed: {failed_test} — {detail}")
}

pub fn run_agent(
    manifest: &AgentManifest,
    session: &SessionLog,
    provider: &dyn ChatProvider,
    registry: &ToolRegistry,
    user_prompt: &str,
    options: RunOptions,
) -> AgentResult<RunOutcome> {
    let mut events = session
        .replay()
        .map_err(|err| AgentError::session(err.to_string()))?;
    if options.fresh_run && !options.resume_only && !events.is_empty() {
        return Err(AgentError::resume(
            "session already has events; use --resume or a new session path",
        ));
    }

    if events.is_empty() {
        session
            .append(&Event::SystemPrompt {
                content: compose_system_prompt(manifest),
            })
            .map_err(|err| AgentError::session(err.to_string()))?;
        session
            .append(&Event::UserMessage {
                content: user_prompt.to_owned(),
            })
            .map_err(|err| AgentError::session(err.to_string()))?;
        events = session
            .replay()
            .map_err(|err| AgentError::session(err.to_string()))?;
    } else if !options.resume_only {
        session
            .append(&Event::UserMessage {
                content: user_prompt.to_owned(),
            })
            .map_err(|err| AgentError::session(err.to_string()))?;
        events = session
            .replay()
            .map_err(|err| AgentError::session(err.to_string()))?;
    }

    if SessionLog::terminal_state(&events) {
        return Ok(RunOutcome {
            diagnosis: session.diagnosis(&events),
            structured: session.structured_diagnosis(&events),
            turns: 0,
        });
    }

    let tool_schemas = if options.rookrunner_run_dir.is_some() {
        ToolRegistry::tool_schemas_rookrunner_run_dir(manifest)
    } else if let Some(names) = &options.exposed_tools {
        ToolRegistry::tool_schemas_filtered(manifest, names)
    } else {
        ToolRegistry::tool_schemas(manifest)
    };
    let workspace_root = options
        .workspace_root
        .clone()
        .unwrap_or_else(|| manifest.root.clone());
    let ctx = ToolContext {
        secrets: std::collections::HashMap::new(),
        command_timeout: manifest.config.command_timeout(),
        skills: manifest.skills.clone(),
        workspace_root,
        rookrunner_run_dir: options.rookrunner_run_dir.clone(),
    };
    let max_turns = manifest.config.effective_max_turns();
    let mut turns = 0u32;

    while turns < max_turns {
        let messages = SessionLog::messages_from_events(&events);
        let response = provider
            .complete(&manifest.config.model, &messages, &tool_schemas)
            .map_err(|err| AgentError::provider(err.to_string()))?;
        turns += 1;

        if !response.tool_calls.is_empty() {
            for call in response.tool_calls {
                session
                    .append(&Event::ToolCall {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        arguments: call.arguments.clone(),
                    })
                    .map_err(|err| AgentError::session(err.to_string()))?;
                let started = Instant::now();
                let dispatch = registry.dispatch(&call.name, &call.arguments, &ctx);
                let duration_ms = started.elapsed().as_millis() as u64;
                match dispatch {
                    Ok(result) => {
                        session
                            .append(&Event::ToolAudit {
                                name: call.name.clone(),
                                ok: true,
                                duration_ms,
                                error_kind: None,
                            })
                            .map_err(|err| AgentError::session(err.to_string()))?;
                        session
                            .append(&Event::ToolResult {
                                call_id: call.id.clone(),
                                name: call.name.clone(),
                                content: result,
                            })
                            .map_err(|err| AgentError::session(err.to_string()))?;
                    }
                    Err(err) => {
                        session
                            .append(&Event::ToolAudit {
                                name: call.name.clone(),
                                ok: false,
                                duration_ms,
                                error_kind: Some(ErrorKind::Tool),
                            })
                            .map_err(|err| AgentError::session(err.to_string()))?;
                        session
                            .append(&Event::ToolResult {
                                call_id: call.id.clone(),
                                name: call.name.clone(),
                                content: format!("error: {err}"),
                            })
                            .map_err(|err| AgentError::session(err.to_string()))?;
                    }
                }
            }
            events = session
                .replay()
                .map_err(|err| AgentError::session(err.to_string()))?;
            continue;
        }

        if !response.message.is_empty() {
            session
                .append(&Event::AssistantMessage {
                    content: response.message.clone(),
                })
                .map_err(|err| AgentError::session(err.to_string()))?;
            if looks_like_diagnosis(&response.message) {
                session
                    .append(&Event::Diagnosis {
                        content: response.message.clone(),
                    })
                    .map_err(|err| AgentError::session(err.to_string()))?;
                if let Some(parsed) = parse_diagnosis_line(&response.message) {
                    let excerpt = last_tool_log_excerpt(&events);
                    let record = enrich_diagnosis(parsed, &options.failed_jobs_hint, excerpt);
                    session
                        .append(&Event::StructuredDiagnosis {
                            record: record.clone(),
                        })
                        .map_err(|err| AgentError::session(err.to_string()))?;
                }
            }
            events = session
                .replay()
                .map_err(|err| AgentError::session(err.to_string()))?;
        }
        break;
    }

    if turns >= max_turns && !SessionLog::terminal_state(&events) {
        return Err(AgentError::turn_limit(max_turns));
    }

    Ok(RunOutcome {
        diagnosis: session.diagnosis(&events),
        structured: session.structured_diagnosis(&events),
        turns,
    })
}

fn last_tool_log_excerpt(events: &[Event]) -> Option<String> {
    events.iter().rev().find_map(|event| match event {
        Event::ToolResult { content, .. } => Some(truncate_excerpt(content)),
        _ => None,
    })
}

fn truncate_excerpt(content: &str) -> String {
    const LIMIT: usize = 400;
    if content.len() <= LIMIT {
        return content.to_owned();
    }
    content.chars().take(LIMIT).collect()
}

fn looks_like_diagnosis(message: &str) -> bool {
    message.contains("DIAGNOSIS:") || message.lines().any(|line| line.starts_with("Root cause:"))
}

pub fn mock_ci_triage_script() -> Vec<ProviderResponse> {
    use crate::provider::ToolCall;
    vec![
        ProviderResponse {
            message: String::new(),
            tool_calls: vec![ToolCall {
                id: "call-1".to_owned(),
                name: "list_failed_jobs".to_owned(),
                arguments: serde_json::json!({ "path": "fixtures/jobs.json" }),
            }],
        },
        ProviderResponse {
            message: String::new(),
            tool_calls: vec![ToolCall {
                id: "call-2".to_owned(),
                name: "read_log".to_owned(),
                arguments: serde_json::json!({ "path": "fixtures/ci.log" }),
            }],
        },
        ProviderResponse {
            message: mock_diagnosis_from_fixture_log(),
            tool_calls: Vec::new(),
        },
    ]
}

fn mock_diagnosis_from_fixture_log() -> String {
    mock_diagnosis_from_log(include_str!("../../fixtures/ci.log"))
}

pub fn mock_rookrunner_triage_script(_run_dir: &str) -> Vec<ProviderResponse> {
    use crate::provider::ToolCall;
    vec![
        ProviderResponse {
            message: String::new(),
            tool_calls: vec![ToolCall {
                id: "call-1".to_owned(),
                name: "rookrunner_list_failed".to_owned(),
                arguments: serde_json::json!({}),
            }],
        },
        ProviderResponse {
            message: String::new(),
            tool_calls: vec![ToolCall {
                id: "call-2".to_owned(),
                name: "rookrunner_read_job_log".to_owned(),
                arguments: serde_json::json!({ "job_name": "test" }),
            }],
        },
        ProviderResponse {
            message: mock_diagnosis_from_log(include_str!(
                "../../fixtures/rookrunner-run/logs/test.log"
            )),
            tool_calls: Vec::new(),
        },
    ]
}

pub use crate::rookrunner::RUN_DIR_TOOL_NAMES as ROOKRUNNER_RUN_DIR_TOOLS;
