use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use cron::Schedule;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use walkdir::WalkDir;

#[derive(Debug, Clone, Deserialize)]
pub struct ScheduleSpec {
    pub id: String,
    pub cron: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScheduleState {
    #[serde(default)]
    pub last_run: HashMap<String, DateTime<Utc>>,
}

pub fn discover_schedules(schedules_dir: &Path) -> Result<Vec<ScheduleSpec>> {
    if !schedules_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut specs = Vec::new();
    for entry in WalkDir::new(schedules_dir).min_depth(1).max_depth(1) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if !path.extension().is_some_and(|ext| ext == "toml") {
            continue;
        }
        let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let spec: ScheduleSpec =
            toml::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;
        if spec.id.is_empty() {
            bail!("schedule {} must set id", path.display());
        }
        specs.push(spec);
    }
    specs.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(specs)
}

pub fn load_state(path: &Path) -> Result<ScheduleState> {
    if !path.is_file() {
        return Ok(ScheduleState::default());
    }
    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))
}

pub fn save_state(path: &Path, state: &ScheduleState) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_string_pretty(state)?;
    fs::write(path, raw).with_context(|| format!("write {}", path.display()))
}

pub fn is_due(
    spec: &ScheduleSpec,
    last_run: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Result<bool> {
    let schedule =
        Schedule::from_str(&spec.cron).with_context(|| format!("parse cron for {}", spec.id))?;
    let start = last_run.unwrap_or_else(|| now - chrono::Duration::days(400));
    let next = schedule.after(&start).next();
    Ok(next.is_some_and(|fire| fire <= now))
}

pub fn due_schedules(
    specs: &[ScheduleSpec],
    state: &ScheduleState,
    now: DateTime<Utc>,
) -> Result<Vec<ScheduleSpec>> {
    let mut due = Vec::new();
    for spec in specs {
        let last = state.last_run.get(&spec.id).copied();
        if is_due(spec, last, now)? {
            due.push(spec.clone());
        }
    }
    Ok(due)
}

pub fn default_state_path(agent_root: &Path) -> PathBuf {
    agent_root.join("out").join("schedule-state.json")
}
