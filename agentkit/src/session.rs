use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::provider::ChatMessage;

fn default_tool_name() -> String {
    "unknown".to_owned()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    SystemPrompt {
        content: String,
    },
    UserMessage {
        content: String,
    },
    AssistantMessage {
        content: String,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    ToolResult {
        call_id: String,
        #[serde(default = "default_tool_name")]
        name: String,
        content: String,
    },
    Diagnosis {
        content: String,
    },
    StructuredDiagnosis {
        record: crate::diagnosis::DiagnosisRecord,
    },
    ToolAudit {
        name: String,
        ok: bool,
        duration_ms: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        error_kind: Option<crate::error::ErrorKind>,
    },
}

enum SessionBackend {
    Jsonl {
        path: PathBuf,
    },
    #[cfg(feature = "sqlite-sessions")]
    Sqlite(crate::session_sqlite::SqliteSession),
}

impl std::fmt::Debug for SessionBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionBackend::Jsonl { path } => f.debug_tuple("Jsonl").field(path).finish(),
            #[cfg(feature = "sqlite-sessions")]
            SessionBackend::Sqlite(_) => f.write_str("Sqlite"),
        }
    }
}

#[derive(Debug)]
pub struct SessionLog {
    backend: SessionBackend,
}

impl SessionLog {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(Self {
            backend: SessionBackend::Jsonl {
                path: path.to_path_buf(),
            },
        })
    }

    #[cfg(feature = "sqlite-sessions")]
    pub fn open_sqlite(path: &Path) -> Result<Self> {
        Ok(Self {
            backend: SessionBackend::Sqlite(crate::session_sqlite::SqliteSession::open(path)?),
        })
    }

    pub fn append(&self, event: &Event) -> Result<()> {
        match &self.backend {
            SessionBackend::Jsonl { path } => append_jsonl(path, event),
            #[cfg(feature = "sqlite-sessions")]
            SessionBackend::Sqlite(store) => store.append(event),
        }
    }

    pub fn replay(&self) -> Result<Vec<Event>> {
        match &self.backend {
            SessionBackend::Jsonl { path } => replay_jsonl(path),
            #[cfg(feature = "sqlite-sessions")]
            SessionBackend::Sqlite(store) => store.replay(),
        }
    }

    pub fn messages_from_events(events: &[Event]) -> Vec<ChatMessage> {
        let mut messages = Vec::new();
        for event in events {
            match event {
                Event::SystemPrompt { content } => {
                    messages.push(ChatMessage::system(content));
                }
                Event::UserMessage { content } => {
                    messages.push(ChatMessage::user(content));
                }
                Event::AssistantMessage { content } => {
                    messages.push(ChatMessage::assistant(content));
                }
                Event::ToolCall {
                    id,
                    name,
                    arguments,
                } => {
                    messages.push(ChatMessage::assistant_tool_call(
                        id.clone(),
                        name,
                        arguments.clone(),
                    ));
                }
                Event::ToolResult {
                    call_id,
                    name,
                    content,
                } => {
                    messages.push(ChatMessage::tool_result(call_id.clone(), name, content));
                }
                Event::Diagnosis { .. }
                | Event::StructuredDiagnosis { .. }
                | Event::ToolAudit { .. } => {}
            }
        }
        messages
    }

    pub fn diagnosis(&self, events: &[Event]) -> Option<String> {
        events.iter().rev().find_map(|event| match event {
            Event::Diagnosis { content } => Some(content.clone()),
            Event::StructuredDiagnosis { record } => Some(record.summary.clone()),
            _ => None,
        })
    }

    pub fn structured_diagnosis(
        &self,
        events: &[Event],
    ) -> Option<crate::diagnosis::DiagnosisRecord> {
        events.iter().rev().find_map(|event| match event {
            Event::StructuredDiagnosis { record } => Some(record.clone()),
            _ => None,
        })
    }

    pub fn terminal_state(events: &[Event]) -> bool {
        matches!(
            events.last(),
            Some(Event::Diagnosis { .. }) | Some(Event::StructuredDiagnosis { .. })
        )
    }
}

fn append_jsonl(path: &Path, event: &Event) -> Result<()> {
    let line = serde_json::to_string(event)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("open session log {}", path.display()))?;
    writeln!(file, "{line}")?;
    Ok(())
}

fn replay_jsonl(path: &Path) -> Result<Vec<Event>> {
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();
    for (index, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("read line {}", index + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        let event: Event = serde_json::from_str(&line)
            .with_context(|| format!("parse event line {}", index + 1))?;
        events.push(event);
    }
    Ok(events)
}

pub fn allocate_session_path(base: &Path, resume: bool) -> Result<PathBuf> {
    if resume {
        return Ok(base.to_path_buf());
    }
    if !base.is_file() {
        return Ok(base.to_path_buf());
    }
    if SessionLog::open(base)?.replay()?.is_empty() {
        return Ok(base.to_path_buf());
    }
    let parent = base
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let stem = base
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("session");
    for index in 1..1000 {
        let candidate = parent.join(format!("{stem}-{index}.jsonl"));
        if !candidate.is_file() {
            return Ok(candidate);
        }
        if SessionLog::open(&candidate)?.replay()?.is_empty() {
            return Ok(candidate);
        }
    }
    anyhow::bail!("too many session files for {}", base.display());
}
