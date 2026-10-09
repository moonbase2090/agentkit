use std::env;
use std::path::PathBuf;

use agentkit::runtime::{mock_ci_triage_script, RunOptions};
use agentkit::schedule::{default_state_path, due_schedules, load_state, save_state};
use agentkit::tools::register_ci_builtins;
use agentkit::{load_agent, run_agent, SessionLog, ToolRegistry};
use chrono::{DateTime, Utc};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    let agent_root = PathBuf::from("agents/ci-triage");
    let manifest = load_agent(&agent_root)?;

    let now = args
        .iter()
        .position(|arg| arg == "--now")
        .and_then(|index| args.get(index + 1))
        .map(|value| DateTime::parse_from_rfc3339(value).map(|dt| dt.with_timezone(&Utc)))
        .transpose()?
        .unwrap_or_else(Utc::now);

    let state_path = default_state_path(&agent_root);
    let mut state = load_state(&state_path)?;
    let due = due_schedules(&manifest.schedules, &state, now)?;
    let ran_any = !due.is_empty();

    let mut registry = ToolRegistry::from_manifest(&manifest);
    register_ci_builtins(&mut registry);
    let provider = agentkit::MockProvider::scripted(mock_ci_triage_script());

    for spec in &due {
        let session_path = agent_root
            .join("out")
            .join("schedules")
            .join(format!("{}.jsonl", spec.id));
        let session = SessionLog::open(&session_path)?;
        let _outcome = run_agent(
            &manifest,
            &session,
            &provider,
            &registry,
            &spec.prompt,
            RunOptions::default(),
        )?;
        state.last_run.insert(spec.id.clone(), now);
        println!("ran schedule {}", spec.id);
    }

    if !ran_any {
        println!("no schedules due");
    }

    save_state(&state_path, &state)?;
    Ok(())
}
