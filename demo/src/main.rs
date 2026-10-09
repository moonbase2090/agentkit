use std::env;
use std::path::PathBuf;

use agentkit::rookrunner;
use agentkit::runtime::{mock_ci_triage_script, mock_rookrunner_triage_script, RunOptions};
use agentkit::session::{allocate_session_path, SessionLog};
use agentkit::tools::register_ci_builtins;
use agentkit::{load_agent, run_agent, OpenRouterProvider, ToolRegistry};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    let resume_only = args.iter().any(|arg| arg == "--resume");
    let live = args.iter().any(|arg| arg == "--live");
    let run_dir = args
        .iter()
        .position(|arg| arg == "--run-dir")
        .and_then(|index| args.get(index + 1))
        .map(PathBuf::from);

    let workspace_root = env::current_dir()?;
    let agent_root = workspace_root.join("agents/ci-triage");
    let manifest = load_agent(&agent_root)?;

    let base_session_path = if let Some(run_dir) = &run_dir {
        let jobs = rookrunner::load_run_jobs(run_dir)?;
        let run_id = jobs.workflow_run_id.as_deref().unwrap_or("unknown-run");
        rookrunner::session_path_for_run(&workspace_root.join("out/sessions"), run_id)
    } else {
        workspace_root.join("out/session.jsonl")
    };
    let session_path = allocate_session_path(&base_session_path, resume_only)?;
    if let Some(parent) = session_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let session = SessionLog::open(&session_path)?;

    let mut registry = ToolRegistry::from_manifest(&manifest);
    register_ci_builtins(&mut registry);

    let failed_jobs_hint = run_dir
        .as_ref()
        .map(|dir| {
            rookrunner::list_failed_jobs(dir)
                .unwrap_or_default()
                .into_iter()
                .map(|job| job.name)
                .collect()
        })
        .unwrap_or_default();

    let user_prompt = if run_dir.is_some() {
        "Triage the CI failure using rookrunner_list_failed and rookrunner_read_job_log on the run directory."
    } else {
        "Triage the fixture CI failure."
    };

    let options = RunOptions {
        resume_only,
        failed_jobs_hint,
        fresh_run: true,
        exposed_tools: None,
        workspace_root: Some(workspace_root.clone()),
        rookrunner_run_dir: run_dir.clone(),
    };

    if live {
        let provider = OpenRouterProvider::from_env(&manifest.config.openrouter_base)?;
        let outcome = run_agent(
            &manifest,
            &session,
            &provider,
            &registry,
            user_prompt,
            options,
        )?;
        println!("turns={} diagnosis={:?}", outcome.turns, outcome.diagnosis);
        return Ok(());
    }

    let provider = if let Some(run_dir) = run_dir {
        agentkit::MockProvider::scripted(mock_rookrunner_triage_script(
            run_dir.to_string_lossy().as_ref(),
        ))
    } else {
        agentkit::MockProvider::scripted(mock_ci_triage_script())
    };

    let outcome = run_agent(
        &manifest,
        &session,
        &provider,
        &registry,
        user_prompt,
        options,
    )?;
    println!("turns={} diagnosis={:?}", outcome.turns, outcome.diagnosis);
    if let Some(diagnosis) = outcome.diagnosis {
        if let Some(parent) = session_path.parent() {
            std::fs::write(parent.join("diagnosis.txt"), diagnosis)?;
        } else {
            std::fs::write(workspace_root.join("out/diagnosis.txt"), diagnosis)?;
        }
    }
    if let Some(record) = outcome.structured {
        if let Some(parent) = session_path.parent() {
            let json = serde_json::to_string_pretty(&record)?;
            std::fs::write(parent.join("diagnosis.json"), json)?;
        }
    }
    Ok(())
}
