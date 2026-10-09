use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosisRecord {
    pub summary: String,
    #[serde(default)]
    pub failed_jobs: Vec<String>,
    #[serde(default)]
    pub log_excerpt: Option<String>,
}

pub fn parse_diagnosis_line(message: &str) -> Option<DiagnosisRecord> {
    let summary = message
        .lines()
        .find(|line| line.starts_with("DIAGNOSIS:"))
        .map(|line| line.trim().to_owned())
        .or_else(|| {
            message
                .lines()
                .find(|line| line.starts_with("Root cause:"))
                .map(|line| {
                    line.replacen("Root cause:", "DIAGNOSIS:", 1)
                        .trim()
                        .to_owned()
                })
        })?;
    Some(DiagnosisRecord {
        summary,
        failed_jobs: Vec::new(),
        log_excerpt: None,
    })
}

pub fn enrich_diagnosis(
    record: DiagnosisRecord,
    failed_jobs: &[String],
    excerpt: Option<String>,
) -> DiagnosisRecord {
    DiagnosisRecord {
        summary: record.summary,
        failed_jobs: if record.failed_jobs.is_empty() {
            failed_jobs.to_vec()
        } else {
            record.failed_jobs
        },
        log_excerpt: record.log_excerpt.or(excerpt),
    }
}
