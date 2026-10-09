mod agent;
pub mod diagnosis;
pub mod error;
mod provider;
pub mod rookrunner;
pub mod runtime;
pub mod schedule;
pub mod session;
#[cfg(feature = "sqlite-sessions")]
mod session_sqlite;
pub mod skills;
pub mod tools;

pub use agent::{compose_system_prompt, load_agent, AgentConfig, AgentManifest};
pub use diagnosis::DiagnosisRecord;
pub use error::{AgentError, AgentResult, ErrorKind};
pub use provider::{
    encode_openrouter_message, openrouter_chat_completion_body, ChatMessage, ChatProvider,
    MockProvider, OpenRouterProvider, ProviderResponse, ToolCall,
};
pub use runtime::{
    mock_diagnosis_from_log, run_agent, RunOptions, RunOutcome, ROOKRUNNER_RUN_DIR_TOOLS,
};
pub use schedule::{
    default_state_path, discover_schedules, due_schedules, is_due, load_state, save_state,
    ScheduleSpec, ScheduleState,
};
pub use session::{allocate_session_path, Event, SessionLog};
pub use skills::SkillEntry;
pub use tools::{ToolContext, ToolRegistry};
