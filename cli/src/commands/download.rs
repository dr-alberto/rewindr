use crate::artifacts;
use crate::auth;
use crate::github::{self, Client};

pub fn run(target: String, repo: Option<String>) {
  let client = Client::new(auth::require_token());
  let repo = github::require_repo(repo);

  let run_id = match artifacts::resolve_run_id(&client, &repo, &target) {
    Ok(run_id) => run_id,
    Err(e) => {
      eprintln!("{e}");
      std::process::exit(1);
    }
  };

  let dir = match artifacts::cache_dir(&repo, run_id) {
    Ok(dir) => dir,
    Err(e) => {
      eprintln!("{e}");
      std::process::exit(1);
    }
  };

  if artifacts::is_populated(&dir) {
    println!("✓ Run {run_id} already cached at {}", dir.display());
    return;
  }

  println!("▸ Downloading rewindr artifact for run {run_id} ...");
  match artifacts::ensure_cached(&client, &repo, run_id) {
    Ok(dir) => println!("✓ Cached at {}", dir.display()),
    Err(e) => {
      eprintln!("Download failed: {e}");
      std::process::exit(1);
    }
  }
}
