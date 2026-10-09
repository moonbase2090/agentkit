use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde_json::json;

use crate::agent::{AgentManifest, ToolImplementation};
use crate::rookrunner;
use crate::skills::SkillEntry;

type BuiltinHandler = Box<dyn Fn(&serde_json::Value, &ToolContext) -> Result<String> + Send + Sync>;

pub struct ToolContext {
    pub secrets: HashMap<String, String>,
    pub command_timeout: Duration,
    pub skills: Vec<SkillEntry>,
    pub workspace_root: PathBuf,
    pub rookrunner_run_dir: Option<PathBuf>,
}

pub struct ToolRegistry {
    builtins: HashMap<String, BuiltinHandler>,
    manifest_tools: Vec<(String, ToolImplementation)>,
}

impl ToolRegistry {
    pub fn from_manifest(manifest: &AgentManifest) -> Self {
        let registry = Self {
            builtins: HashMap::new(),
            manifest_tools: manifest
                .tools
                .iter()
                .map(|tool| (tool.name.clone(), tool.implementation.clone()))
                .collect(),
        };
        registry
    }

    pub fn register_builtin(
        &mut self,
        id: &str,
        handler: impl Fn(&serde_json::Value, &ToolContext) -> Result<String> + Send + Sync + 'static,
    ) {
        self.builtins.insert(id.to_owned(), Box::new(handler));
    }

    pub fn tool_schemas_filtered(
        manifest: &AgentManifest,
        names: &[String],
    ) -> Vec<serde_json::Value> {
        manifest
            .tools
            .iter()
            .filter(|tool| names.iter().any(|name| name == &tool.name))
            .map(tool_schema_json)
            .collect()
    }

    pub fn tool_schemas(manifest: &AgentManifest) -> Vec<serde_json::Value> {
        manifest.tools.iter().map(tool_schema_json).collect()
    }

    pub fn tool_schemas_rookrunner_run_dir(manifest: &AgentManifest) -> Vec<serde_json::Value> {
        let names: Vec<String> = rookrunner::RUN_DIR_TOOL_NAMES
            .iter()
            .map(|name| name.to_string())
            .collect();
        Self::tool_schemas_filtered(manifest, &names)
            .into_iter()
            .map(strip_run_dir_from_tool_schema)
            .collect()
    }

    pub fn dispatch(
        &self,
        name: &str,
        arguments: &serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<String> {
        let implementation = self
            .manifest_tools
            .iter()
            .find(|(tool_name, _)| tool_name == name)
            .map(|(_, implementation)| implementation)
            .context("unknown tool")?;

        match implementation {
            ToolImplementation::Builtin(id) => {
                let handler = self
                    .builtins
                    .get(id)
                    .with_context(|| format!("missing builtin handler for {id}"))?;
                handler(arguments, ctx)
            }
            ToolImplementation::Command {
                program,
                env_secret,
            } => run_command_tool(program, env_secret, arguments, ctx),
        }
    }
}

fn strip_run_dir_from_tool_schema(schema: serde_json::Value) -> serde_json::Value {
    let mut schema = schema;
    if let Some(function) = schema.get_mut("function") {
        if let Some(parameters) = function.get_mut("parameters") {
            if let Some(properties) = parameters.get_mut("properties") {
                if let Some(props) = properties.as_object_mut() {
                    props.remove("run_dir");
                }
            }
            if let Some(required) = parameters.get_mut("required") {
                if let Some(items) = required.as_array_mut() {
                    items.retain(|item| item.as_str() != Some("run_dir"));
                }
            }
        }
    }
    schema
}

fn rookrunner_run_dir(ctx: &ToolContext) -> Result<&Path> {
    ctx.rookrunner_run_dir
        .as_deref()
        .context("rookrunner run directory is not configured")
}

fn tool_schema_json(tool: &crate::agent::ToolDef) -> serde_json::Value {
    json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.input_schema,
        }
    })
}

fn resolve_workspace_path(ctx: &ToolContext, path: &str) -> PathBuf {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        return candidate.to_path_buf();
    }
    ctx.workspace_root.join(candidate)
}

fn run_command_tool(
    program: &PathBuf,
    env_secret: &Option<String>,
    arguments: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String> {
    let mut command = Command::new(program);
    command.stdin(Stdio::piped()).stdout(Stdio::piped());
    if let Some(env_name) = env_secret {
        let value = ctx
            .secrets
            .get(env_name)
            .cloned()
            .or_else(|| std::env::var(env_name).ok())
            .context("missing secret env for command tool")?;
        command.env(env_name, value);
    }
    let mut child = command.spawn().context("spawn command tool")?;
    if let Some(mut stdin) = child.stdin.take() {
        let payload = serde_json::to_vec(arguments)?;
        stdin.write_all(&payload)?;
    }
    let deadline = Instant::now() + ctx.command_timeout;
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().context("poll command tool")? {
            let output = child
                .wait_with_output()
                .context("read command tool output")?;
            if !status.success() {
                bail!(
                    "command tool failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            return Ok(String::from_utf8_lossy(&output.stdout).to_string());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    child.kill().ok();
    bail!(
        "command tool timed out after {}s",
        ctx.command_timeout.as_secs()
    );
}

pub fn builtin_read_log(args: &serde_json::Value, ctx: &ToolContext) -> Result<String> {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .context("read_log requires path")?;
    let resolved = resolve_workspace_path(ctx, path);
    fs::read_to_string(&resolved).with_context(|| format!("read log {}", resolved.display()))
}

pub fn builtin_list_failed_jobs(args: &serde_json::Value, ctx: &ToolContext) -> Result<String> {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .context("list_failed_jobs requires path")?;
    let resolved = resolve_workspace_path(ctx, path);
    let raw = fs::read_to_string(&resolved)
        .with_context(|| format!("read jobs {}", resolved.display()))?;
    let jobs: serde_json::Value = serde_json::from_str(&raw)?;
    let failed = jobs
        .get("jobs")
        .and_then(|v| v.as_array())
        .map(|jobs| {
            jobs.iter()
                .filter(|job| job.get("conclusion").and_then(|v| v.as_str()) == Some("failure"))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(serde_json::to_string(&failed)?)
}

pub fn builtin_rookrunner_list_failed(
    _args: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String> {
    let run_dir = rookrunner_run_dir(ctx)?;
    let failed = rookrunner::list_failed_jobs(run_dir)?;
    Ok(serde_json::to_string(&failed)?)
}

pub fn builtin_rookrunner_read_job_log(
    args: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String> {
    let run_dir = rookrunner_run_dir(ctx)?;
    let job_name = args
        .get("job_name")
        .and_then(|v| v.as_str())
        .context("rookrunner_read_job_log requires job_name")?;
    rookrunner::read_job_log(run_dir, job_name)
}

pub fn builtin_load_skill(args: &serde_json::Value, ctx: &ToolContext) -> Result<String> {
    let skill_id = args
        .get("skill_id")
        .and_then(|v| v.as_str())
        .context("load_skill requires skill_id")?;
    crate::skills::load_skill_body(&ctx.skills, skill_id)
}

pub fn register_ci_builtins(registry: &mut ToolRegistry) {
    registry.register_builtin("read_log", builtin_read_log);
    registry.register_builtin("list_failed_jobs", builtin_list_failed_jobs);
    registry.register_builtin("rookrunner_list_failed", builtin_rookrunner_list_failed);
    registry.register_builtin("rookrunner_read_job_log", builtin_rookrunner_read_job_log);
    registry.register_builtin("load_skill", builtin_load_skill);
}
