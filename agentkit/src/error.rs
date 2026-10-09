use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Config,
    Session,
    Provider,
    Tool,
    TurnLimit,
    Resume,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentError {
    pub kind: ErrorKind,
    pub message: String,
}

impl AgentError {
    pub fn config(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Config,
            message: message.into(),
        }
    }

    pub fn session(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Session,
            message: message.into(),
        }
    }

    pub fn provider(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Provider,
            message: message.into(),
        }
    }

    pub fn tool(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Tool,
            message: message.into(),
        }
    }

    pub fn turn_limit(max: u32) -> Self {
        Self {
            kind: ErrorKind::TurnLimit,
            message: format!("exceeded max_turns ({max})"),
        }
    }

    pub fn resume(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Resume,
            message: message.into(),
        }
    }
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for AgentError {}

pub type AgentResult<T> = Result<T, AgentError>;
