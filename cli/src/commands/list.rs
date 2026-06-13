use std::collections::HashMap;

use crate::auth;
use crate::github::{self, ARTIFACT_PREFIX, Client};

/// Runs and artifacts are fetched a page at a time; this is GitHub's maximum.
const PER_PAGE: &str = "100";

pub fn run(limit: u32, workflow: Option<String>, repo: Option<String>) {
  let token = match auth::token() {
    Ok(Some(token)) => token,
    Ok(None) => {
      eprintln!("Not authenticated. Run `rewindr login` first.");
      std::process::exit(1);
    }
    Err(e) => {
      eprintln!("Failed to read stored token: {e}");
      std::process::exit(1);
    }
  };

  let repo = match repo.or_else(github::detect_repo) {
    Some(repo) => repo,
    None => {
      eprintln!("Could not detect repository. Pass --repo owner/repo.");
      std::process::exit(1);
    }
  };

  let client = Client::new(token);

  let runs_path = match &workflow {
    Some(workflow) => format!("/repos/{repo}/actions/workflows/{workflow}/runs"),
    None => format!("/repos/{repo}/actions/runs"),
  };
  let runs: github::WorkflowRuns = match client.get(&runs_path, &[("per_page", PER_PAGE)]) {
    Ok(runs) => runs,
    Err(e) => {
      eprintln!("Failed to fetch workflow runs: {e}");
      if workflow.is_some() {
        eprintln!("Check that the --workflow file name is correct (e.g. build-and-test.yml).");
      }
      std::process::exit(1);
    }
  };
  let runs = runs.workflow_runs;

  // One repo-wide artifacts request, then join to runs in memory — avoids a
  // request per run. Artifacts come newest-first, so the first match per run
  // wins.
  let artifacts: github::Artifacts =
    match client.get(&format!("/repos/{repo}/actions/artifacts"), &[("per_page", PER_PAGE)]) {
      Ok(artifacts) => artifacts,
      Err(e) => {
        eprintln!("Failed to fetch artifacts: {e}");
        std::process::exit(1);
      }
    };

  let mut artifact_by_run: HashMap<u64, &str> = HashMap::new();
  for artifact in &artifacts.artifacts {
    if !artifact.name.starts_with(ARTIFACT_PREFIX) {
      continue;
    }
    if let Some(run) = &artifact.workflow_run {
      artifact_by_run.entry(run.id).or_insert(&artifact.name);
    }
  }

  println!();
  println!(
    "{:<14} {:<12} {:<12} {:<20} {:<22} ARTIFACT",
    "RUN ID", "STATUS", "CONCLUSION", "BRANCH", "DATE"
  );
  println!("{}", "-".repeat(100));

  let mut shown = 0;
  for run in &runs {
    if shown >= limit {
      break;
    }
    let Some(artifact) = artifact_by_run.get(&run.id) else {
      continue;
    };

    let conclusion = run.conclusion.as_deref().unwrap_or("—");
    let branch = truncate(run.head_branch.as_deref().unwrap_or(""), 19);
    let date = truncate(run.created_at.as_deref().unwrap_or(""), 19).replace('T', " ");

    println!(
      "{:<14} {:<12} {:<12} {:<20} {:<22} {}",
      run.id, run.status, conclusion, branch, date, artifact
    );
    shown += 1;
  }

  if shown == 0 {
    println!("  (no runs with rewindr artifacts found in the last {} runs)", runs.len());
  }
  println!();
}

/// Truncate a string to at most `max` characters.
fn truncate(s: &str, max: usize) -> String {
  s.chars().take(max).collect()
}
