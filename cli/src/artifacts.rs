//! Local cache of downloaded rewindr artifacts, modelled on `docker pull`:
//! each run's artifact is fetched once into a per-repo, per-run directory and
//! replayed from there afterwards.

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use crate::github::{self, Client, ARTIFACT_PREFIX};

/// Files every rewindr artifact carries (see src/post.js).
pub const ARTIFACT_FILES: [&str; 3] =
    ["rewindr.json", "env_dump.txt", "workspace_dump.tar.gz"];

/// Cache directory for a run: `<data_dir>/rewindr/<owner>/<repo>/<run_id>`.
pub fn cache_dir(repo: &str, run_id: u64) -> Result<PathBuf, String> {
    let base = dirs::data_dir().ok_or("could not determine the OS data directory")?;
    Ok(base.join("rewindr").join(repo).join(run_id.to_string()))
}

pub fn is_populated(dir: &Path) -> bool {
    ARTIFACT_FILES.iter().all(|file| dir.join(file).exists())
}

/// Accepts `"latest"` or a numeric run id and resolves it to a concrete run id.
pub fn resolve_run_id(client: &Client, repo: &str, target: &str) -> Result<u64, String> {
    if target.eq_ignore_ascii_case("latest") {
        return github::latest_run_with_artifact(client, repo);
    }
    target
        .parse::<u64>()
        .map_err(|_| format!("invalid run id '{target}' (expected a number or 'latest')"))
}

/// Downloads on a cache miss; returns the populated cache directory.
pub fn ensure_cached(client: &Client, repo: &str, run_id: u64) -> Result<PathBuf, String> {
    let dir = cache_dir(repo, run_id)?;
    if !is_populated(&dir) {
        download_into(client, repo, run_id, &dir)?;
    }
    Ok(dir)
}

fn download_into(client: &Client, repo: &str, run_id: u64, dir: &Path) -> Result<(), String> {
    let artifacts: github::Artifacts =
        client.get(&format!("/repos/{repo}/actions/runs/{run_id}/artifacts"), &[])?;
    let artifact = artifacts
        .artifacts
        .iter()
        .find(|a| a.name.starts_with(ARTIFACT_PREFIX))
        .ok_or_else(|| {
            let names: Vec<&str> = artifacts.artifacts.iter().map(|a| a.name.as_str()).collect();
            format!("no rewindr artifact for run {run_id} (available: {names:?})")
        })?;

    let zip = client.get_bytes(&format!(
        "/repos/{repo}/actions/artifacts/{}/zip",
        artifact.id
    ))?;
    let mut archive =
        zip::ZipArchive::new(Cursor::new(zip)).map_err(|e| format!("reading artifact zip: {e}"))?;

    // Extract to a temporary sibling and rename it into place; an interrupted
    // download leaves nothing partial in the cache.
    let parent = dir.parent().ok_or("invalid cache path")?;
    fs::create_dir_all(parent).map_err(|e| format!("creating {}: {e}", parent.display()))?;
    let staging = parent.join(format!(".{run_id}.partial"));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(|e| format!("creating {}: {e}", staging.display()))?;

    archive
        .extract(&staging)
        .map_err(|e| format!("extracting artifact: {e}"))?;

    let _ = fs::remove_dir_all(dir);
    fs::rename(&staging, dir).map_err(|e| format!("finalizing cache entry: {e}"))
}
