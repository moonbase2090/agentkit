use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct SkillEntry {
    pub id: String,
    pub path: PathBuf,
    pub summary: String,
}

pub fn discover_skills(skills_dir: &Path) -> Result<Vec<SkillEntry>> {
    if !skills_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    for entry in WalkDir::new(skills_dir).min_depth(1).max_depth(1) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if !path.extension().is_some_and(|ext| ext == "md") {
            continue;
        }
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .context("skill file name")?
            .to_owned();
        let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let summary = first_summary_line(&raw);
        entries.push(SkillEntry {
            id,
            path: path.to_path_buf(),
            summary,
        });
    }
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(entries)
}

fn first_summary_line(content: &str) -> String {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let stripped = trimmed.trim_start_matches('#').trim();
        if !stripped.is_empty() {
            return stripped.to_owned();
        }
    }
    String::from("(no summary)")
}

pub fn format_catalog(skills: &[SkillEntry]) -> String {
    let mut lines = vec![
        "## Skills".to_owned(),
        "Use the load_skill tool to read full skill bodies on demand.".to_owned(),
    ];
    for skill in skills {
        lines.push(format!("- {}: {}", skill.id, skill.summary));
    }
    lines.join("\n")
}

pub fn load_skill_body(skills: &[SkillEntry], skill_id: &str) -> Result<String> {
    let entry = skills
        .iter()
        .find(|skill| skill.id == skill_id)
        .with_context(|| format!("unknown skill {skill_id}"))?;
    fs::read_to_string(&entry.path).with_context(|| format!("read {}", entry.path.display()))
}
