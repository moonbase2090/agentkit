use std::fs;
use std::path::Path;

pub const RUN_DIR_TOOL_NAMES: [&str; 2] = ["rookrunner_list_failed", "rookrunner_read_job_log"];

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowJob {
    pub name: String,
    pub conclusion: String,
    #[serde(default)]
    pub log_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRunJobs {
    #[serde(default)]
    pub workflow_run_id: Option<String>,
    pub jobs: Vec<WorkflowJob>,
}

pub fn load_run_jobs(run_dir: &Path) -> Result<WorkflowRunJobs> {
    let jobs_path = run_dir.join("jobs.json");
    let raw =
        fs::read_to_string(&jobs_path).with_context(|| format!("read {}", jobs_path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("parse {}", jobs_path.display()))
}

pub fn list_failed_jobs(run_dir: &Path) -> Result<Vec<WorkflowJob>> {
    let document = load_run_jobs(run_dir)?;
    Ok(document
        .jobs
        .into_iter()
        .filter(|job| job.conclusion == "failure")
        .collect())
}

pub fn read_job_log(run_dir: &Path, job_name: &str) -> Result<String> {
    let document = load_run_jobs(run_dir)?;
    let job = document
        .jobs
        .iter()
        .find(|job| job.name == job_name)
        .with_context(|| format!("unknown job {job_name}"))?;
    let default_relative = default_log_relative(job_name);
    let relative = job.log_file.as_deref().unwrap_or(&default_relative);
    let path = run_dir.join(relative);
    if !path.is_file() {
        bail!("missing log {}", path.display());
    }
    fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))
}

fn default_log_relative(job_name: &str) -> String {
    format!("logs/{}.log", job_name)
}

pub fn session_path_for_run(base: &Path, workflow_run_id: &str) -> std::path::PathBuf {
    base.join(workflow_run_id).join("session.jsonl")
}
