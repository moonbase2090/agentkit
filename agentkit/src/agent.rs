use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use walkdir::WalkDir;

use crate::schedule::{discover_schedules, ScheduleSpec};
use crate::skills::{discover_skills, SkillEntry};

#[derive(Debug, Clone, Deserialize)]
pub struct AgentConfig {
    pub model: String,
    #[serde(default = "default_base")]
    pub openrouter_base: String,
    #[serde(default)]
    pub max_turns: u32,
    #[serde(default = "default_command_timeout_secs")]
    pub command_timeout_secs: u64,
}

fn default_command_timeout_secs() -> u64 {
    30
}

impl AgentConfig {
    pub fn effective_max_turns(&self) -> u32 {
        let configured = if self.max_turns == 0 {
            8
        } else {
            self.max_turns
        };
        configured.min(32)
    }

    pub fn command_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.command_timeout_secs.clamp(1, 300))
    }
}

fn default_base() -> String {
    "https://openrouter.ai/api/v1".to_owned()
}

#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub implementation: ToolImplementation,
}

#[derive(Debug, Clone)]
pub enum ToolImplementation {
    Builtin(String),
    Command {
        program: PathBuf,
        env_secret: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct AgentManifest {
    pub root: PathBuf,
    pub instructions: String,
    pub config: AgentConfig,
    pub tools: Vec<ToolDef>,
    pub skills: Vec<SkillEntry>,
    pub schedules: Vec<ScheduleSpec>,
}

#[derive(Debug, Deserialize)]
struct ToolFile {
    name: String,
    description: String,
    input_schema: serde_json::Value,
    #[serde(flatten)]
    implementation: ToolFileImpl,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
enum ToolFileImpl {
    Builtin {
        id: String,
    },
    Command {
        program: PathBuf,
        #[serde(default)]
        secret_env: Option<String>,
    },
}

pub fn load_agent(root: &Path) -> Result<AgentManifest> {
    let instructions_path = root.join("instructions.md");
    let config_path = root.join("agent.toml");
    if !instructions_path.is_file() {
        bail!("missing instructions.md in {}", root.display());
    }
    if !config_path.is_file() {
        bail!("missing agent.toml in {}", root.display());
    }

    let instructions = fs::read_to_string(&instructions_path)
        .with_context(|| format!("read {}", instructions_path.display()))?;
    let config_text = fs::read_to_string(&config_path)
        .with_context(|| format!("read {}", config_path.display()))?;
    let config: AgentConfig =
        toml::from_str(&config_text).with_context(|| format!("parse {}", config_path.display()))?;

    let tools_dir = root.join("tools");
    let mut tools = Vec::new();
    if tools_dir.is_dir() {
        for entry in WalkDir::new(&tools_dir)
            .min_depth(1)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                let raw =
                    fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
                let file: ToolFile = serde_json::from_str(&raw)
                    .with_context(|| format!("parse {}", path.display()))?;
                validate_tool(&file)?;
                tools.push(ToolDef {
                    name: file.name,
                    description: file.description,
                    input_schema: file.input_schema,
                    implementation: match file.implementation {
                        ToolFileImpl::Builtin { id } => ToolImplementation::Builtin(id),
                        ToolFileImpl::Command {
                            program,
                            secret_env,
                        } => ToolImplementation::Command {
                            program,
                            env_secret: secret_env,
                        },
                    },
                });
            }
        }
    }

    let skills_dir = root.join("skills");
    let skills = discover_skills(&skills_dir)?;
    let schedules_dir = root.join("schedules");
    let schedules = discover_schedules(&schedules_dir)?;

    Ok(AgentManifest {
        root: root.to_path_buf(),
        instructions,
        config,
        tools,
        skills,
        schedules,
    })
}

fn validate_tool(file: &ToolFile) -> Result<()> {
    if file.name.is_empty() {
        bail!("tool name must not be empty");
    }
    if !file.input_schema.is_object() {
        bail!("tool {} input_schema must be a JSON object", file.name);
    }
    Ok(())
}

pub fn compose_system_prompt(manifest: &AgentManifest) -> String {
    let mut parts = vec![manifest.instructions.clone()];
    if !manifest.skills.is_empty() {
        parts.push(String::new());
        parts.push(crate::skills::format_catalog(&manifest.skills));
    }
    parts.join("\n")
}
