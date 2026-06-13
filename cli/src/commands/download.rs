use std::fs;
use std::io::Cursor;
use std::path::PathBuf;

use crate::auth;
use crate::github::{self, ARTIFACT_PREFIX, Client};

pub fn run(run_id: u64, repo: Option<String>, out: Option<String>) {
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

  println!("▸ Fetching artifacts for run {run_id} ...");
  let artifacts: github::Artifacts =
    match client.get(&format!("/repos/{repo}/actions/runs/{run_id}/artifacts"), &[]) {
      Ok(artifacts) => artifacts,
      Err(e) => {
        eprintln!("Failed to fetch artifacts: {e}");
        std::process::exit(1);
      }
    };

  let artifact = match artifacts.artifacts.iter().find(|a| a.name.starts_with(ARTIFACT_PREFIX)) {
    Some(artifact) => artifact,
    None => {
      eprintln!("No rewindr artifact found for run {run_id}.");
      let names: Vec<&str> = artifacts.artifacts.iter().map(|a| a.name.as_str()).collect();
      eprintln!("  Available artifacts: {names:?}");
      std::process::exit(1);
    }
  };

  let out_dir = PathBuf::from(out.unwrap_or_else(|| format!("rewindr-artifacts/{run_id}")));
  if let Err(e) = fs::create_dir_all(&out_dir) {
    eprintln!("Failed to create {}: {e}", out_dir.display());
    std::process::exit(1);
  }

  println!("▸ Downloading '{}' (id={}) ...", artifact.name, artifact.id);
  let zip_bytes =
    match client.get_bytes(&format!("/repos/{repo}/actions/artifacts/{}/zip", artifact.id)) {
      Ok(bytes) => bytes,
      Err(e) => {
        eprintln!("Download failed: {e}");
        std::process::exit(1);
      }
    };

  let mut archive = match zip::ZipArchive::new(Cursor::new(zip_bytes)) {
    Ok(archive) => archive,
    Err(e) => {
      eprintln!("Failed to read artifact zip: {e}");
      std::process::exit(1);
    }
  };
  if let Err(e) = archive.extract(&out_dir) {
    eprintln!("Failed to extract artifact: {e}");
    std::process::exit(1);
  }

  let mut files: Vec<_> = match fs::read_dir(&out_dir) {
    Ok(entries) => entries.flatten().collect(),
    Err(e) => {
      eprintln!("Failed to list {}: {e}", out_dir.display());
      std::process::exit(1);
    }
  };
  files.sort_by_key(|e| e.file_name());

  println!("✓ Extracted {} file(s) to {}/", files.len(), out_dir.display());
  for entry in &files {
    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
    println!("  {}  ({size} bytes)", entry.file_name().to_string_lossy());
  }
}
