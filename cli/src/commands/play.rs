use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Deserialize;

use crate::artifacts;
use crate::auth;
use crate::github::{self, Client};

/// Highest manifest schemaVersion this CLI understands.
const SUPPORTED_SCHEMA: u32 = 1;

/// Env vars that describe the *host* shell rather than the CI run; injecting
/// them would point the container at paths and identities that don't exist.
const ENV_DENYLIST: &[&str] =
  &["PATH", "HOME", "PWD", "OLDPWD", "SHELL", "SHLVL", "USER", "LOGNAME", "HOSTNAME", "TERM", "_"];

/// Where the workspace is mounted when the run's original path is unknown.
const FALLBACK_WORKSPACE: &str = "/workspace";

/// The subset of `rewindr.json` the CLI needs to rebuild the environment.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
  schema_version: u32,
  repository: Option<String>,
  sha: Option<String>,
  workflow: Option<String>,
  job: Option<String>,
  workspace_path: Option<String>,
  #[serde(default)]
  env_captured: bool,
  #[serde(default)]
  secrets_redacted: bool,
  #[serde(default)]
  runner: Runner,
}

#[derive(Deserialize, Default)]
struct Runner {
  #[serde(rename = "imageOS")]
  image_os: Option<String>,
}

pub fn run(
  target: Option<String>,
  repo: Option<String>,
  image: Option<String>,
  build_only: bool,
  dir: Option<String>,
) {
  // Either play an explicit local directory, or resolve the run from the cache
  // (downloading it on a miss). A run id is required.
  let dir = match dir {
    Some(dir) => local_dir(dir),
    None => match target {
      Some(target) => cached_dir(target, repo),
      None => {
        eprintln!("Specify a run id or 'latest' (e.g. `rewindr play latest`), or pass --dir <path>.");
        std::process::exit(1);
      }
    },
  };

  // Refuse anything that isn't a rewindr artifact rather than producing a
  // broken environment from a half-matching directory.
  for file in artifacts::ARTIFACT_FILES {
    if !dir.join(file).exists() {
      eprintln!("{} is not a rewindr artifact (missing {file}).", dir.display());
      eprintln!("Expected files: {}", artifacts::ARTIFACT_FILES.join(", "));
      std::process::exit(1);
    }
  }

  // Fail fast on missing tooling, before doing any work. tar is always needed
  // (we unpack regardless of --build-only); docker only when we actually launch.
  if let Err(e) = ensure_available("tar", &["--version"]) {
    eprintln!("tar is required to unpack the workspace but isn't available: {e}");
    std::process::exit(1);
  }
  if !build_only && ensure_available("docker", &["info"]).is_err() {
    eprintln!("Docker isn't available. Install Docker and make sure the daemon is running,");
    eprintln!("or use --build-only to prepare the environment without launching it.");
    std::process::exit(1);
  }

  let manifest = match load_manifest(&dir) {
    Ok(manifest) => manifest,
    Err(e) => {
      eprintln!("{e}");
      std::process::exit(1);
    }
  };
  print_context(&manifest);

  let workspace = dir.join("workspace");
  println!("▸ Unpacking workspace ...");
  if let Err(e) = unpack_workspace(&dir, &workspace) {
    eprintln!("Failed to unpack workspace: {e}");
    std::process::exit(1);
  }

  let env = load_env(&dir);
  let env_file = dir.join("rewindr.env");
  if let Err(e) = write_env_file(&env, &env_file) {
    eprintln!("Failed to write env file: {e}");
    std::process::exit(1);
  }

  let image = image.unwrap_or_else(|| infer_image(manifest.runner.image_os.as_deref()));

  // Mount the workspace at the run's *original* path so $GITHUB_WORKSPACE and
  // any scripts using absolute runner paths resolve correctly.
  let mount_path = workspace_path(&manifest);
  let workspace = canonical(&workspace);
  let env_file = canonical(&env_file);
  let docker_args = [
    "run", "--rm", "-it",
    "--env-file", &env_file,
    "-v", &format!("{workspace}:{mount_path}"),
    "-w", &mount_path,
    &image,
    "bash",
  ];

  if build_only {
    println!("▸ Environment ready in {}/", dir.display());
    println!("  Run it with:\n    docker {}", docker_args.join(" "));
    return;
  }

  // Pull the image up front so a pull failure (or the large first-time download)
  // is reported clearly, separately from the shell's own exit code.
  if let Err(e) = ensure_image(&image) {
    eprintln!("{e}");
    std::process::exit(1);
  }

  println!("▸ Launching {image} (workspace mounted at {mount_path}) ...\n");
  match Command::new("docker").args(docker_args).status() {
    // A non-zero status here is the shell's own exit code, not our failure.
    Ok(status) => std::process::exit(status.code().unwrap_or(0)),
    Err(e) => {
      eprintln!("Failed to run docker: {e}");
      std::process::exit(1);
    }
  }
}

fn ensure_image(image: &str) -> Result<(), String> {
  if ensure_available("docker", &["image", "inspect", image]).is_ok() {
    return Ok(());
  }
  println!("▸ Pulling {image} ...");
  println!("  (the default catthehacker images are large; the first pull can take a while)");
  let status = Command::new("docker")
    .args(["pull", image])
    .status()
    .map_err(|e| format!("running docker pull: {e}"))?;
  if status.success() {
    Ok(())
  } else {
    Err(format!(
      "Failed to pull {image}. Check the image name (--image) and your network connection."
    ))
  }
}

/// Validate a user-provided local artifact directory (the `--dir` escape hatch).
fn local_dir(dir: String) -> PathBuf {
  let path = PathBuf::from(dir);
  if !path.is_dir() {
    eprintln!("No such directory: {}", path.display());
    std::process::exit(1);
  }
  path
}

/// Resolve the run (id or `latest`) and return its cache directory,
/// downloading the artifact if it isn't cached yet.
fn cached_dir(target: String, repo: Option<String>) -> PathBuf {
  let client = Client::new(auth::require_token());
  let repo = github::require_repo(repo);

  let run_id = artifacts::resolve_run_id(&client, &repo, &target).unwrap_or_else(|e| {
    eprintln!("{e}");
    std::process::exit(1);
  });

  let cache = artifacts::cache_dir(&repo, run_id).unwrap_or_else(|e| {
    eprintln!("{e}");
    std::process::exit(1);
  });
  if !artifacts::is_populated(&cache) {
    println!("▸ Downloading rewindr artifact for run {run_id} ...");
  }

  artifacts::ensure_cached(&client, &repo, run_id).unwrap_or_else(|e| {
    eprintln!("Download failed: {e}");
    std::process::exit(1);
  })
}

/// Read and validate `rewindr.json`, rejecting artifacts newer than this CLI.
fn load_manifest(dir: &Path) -> Result<Manifest, String> {
  let raw = fs::read_to_string(dir.join("rewindr.json"))
    .map_err(|e| format!("reading rewindr.json: {e}"))?;
  let manifest: Manifest =
    serde_json::from_str(&raw).map_err(|e| format!("parsing rewindr.json: {e}"))?;
  if manifest.schema_version > SUPPORTED_SCHEMA {
    return Err(format!(
      "This artifact uses manifest schema v{} but this CLI supports v{SUPPORTED_SCHEMA}. Update rewindr.",
      manifest.schema_version
    ));
  }
  Ok(manifest)
}

fn print_context(manifest: &Manifest) {
  let field = |value: &Option<String>| value.clone().unwrap_or_else(|| "?".to_string());
  let sha = field(&manifest.sha);
  println!(
    "▸ Replaying {} @ {} ({} / {})",
    field(&manifest.repository),
    &sha[..sha.len().min(7)],
    field(&manifest.workflow),
    field(&manifest.job),
  );
  if !manifest.env_captured {
    println!(
      "  ⚠ Environment variables weren't captured (the action ran without the `secrets` input)."
    );
  } else if manifest.secrets_redacted {
    println!("  Environment captured; secret values were redacted.");
  }
}

fn unpack_workspace(dir: &Path, workspace: &Path) -> Result<(), String> {
  if workspace.exists() {
    fs::remove_dir_all(workspace).map_err(|e| e.to_string())?;
  }
  fs::create_dir_all(workspace).map_err(|e| e.to_string())?;

  let status = Command::new("tar")
    .arg("-xzf")
    .arg(dir.join("workspace_dump.tar.gz"))
    .arg("-C")
    .arg(workspace)
    .status()
    .map_err(|e| format!("running tar: {e}"))?;
  if !status.success() {
    return Err("tar exited with an error".to_string());
  }
  Ok(())
}

/// Skips continuation lines of multi-line values (lines without a valid `KEY=` prefix).
fn load_env(dir: &Path) -> Vec<(String, String)> {
  let raw = fs::read_to_string(dir.join("env_dump.txt")).unwrap_or_default();
  raw
    .lines()
    .filter_map(|line| line.split_once('='))
    .filter(|(key, _)| {
      !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
    .map(|(key, value)| (key.to_string(), value.to_string()))
    .collect()
}

/// Write the env vars Docker should inject, dropping host-shell vars.
fn write_env_file(env: &[(String, String)], path: &Path) -> Result<(), String> {
  let body: String = env
    .iter()
    .filter(|(key, _)| !ENV_DENYLIST.contains(&key.as_str()))
    .map(|(key, value)| format!("{key}={value}\n"))
    .collect();
  fs::write(path, body).map_err(|e| e.to_string())
}

/// Pick a base image from the runner's `imageOS`.
///
/// Vanilla `ubuntu:*` images lack all of the tools GitHub preinstalls, so
/// we use the catthehacker runner mirrors (the same images `act` uses) for a
/// faithful rebuild. `--image` overrides this.
fn infer_image(image_os: Option<&str>) -> String {
  match image_os {
    Some("ubuntu24") => "catthehacker/ubuntu:full-24.04",
    Some("ubuntu20") => "catthehacker/ubuntu:full-20.04",
    _ => "catthehacker/ubuntu:full-22.04",
  }
  .to_string()
}

/// The path to mount the workspace at inside the container: the run's original
/// `GITHUB_WORKSPACE`, so absolute runner paths keep resolving.
fn workspace_path(manifest: &Manifest) -> String {
  manifest
    .workspace_path
    .clone()
    .filter(|value| value.starts_with('/'))
    .unwrap_or_else(|| FALLBACK_WORKSPACE.to_string())
}

/// Returns an error if the program isn't found or exits non-zero.
fn ensure_available(program: &str, args: &[&str]) -> Result<(), String> {
  Command::new(program)
    .args(args)
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .status()
    .map_err(|e| e.to_string())
    .and_then(|status| {
      if status.success() {
        Ok(())
      } else {
        Err(format!("`{program}` exited unsuccessfully"))
      }
    })
}

/// Falls back to the original path if canonicalization fails.
fn canonical(path: &Path) -> String {
  fs::canonicalize(path)
    .unwrap_or_else(|_| path.to_path_buf())
    .to_string_lossy()
    .into_owned()
}
