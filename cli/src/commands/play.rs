use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Files every rewindr artifact carries (see src/post.js).
const REQUIRED_FILES: [&str; 3] =
  ["env_dump.txt", "github_context.json", "workspace_dump.tar.gz"];

/// Directory `download` extracts artifacts into, searched when no path is given.
const ARTIFACTS_DIR: &str = "rewindr-artifacts";

/// Env vars that describe the *host* shell rather than the CI run; injecting
/// them would point the container at paths and identities that don't exist.
const ENV_DENYLIST: &[&str] =
  &["PATH", "HOME", "PWD", "OLDPWD", "SHELL", "SHLVL", "USER", "LOGNAME", "HOSTNAME", "TERM", "_"];

/// Where the workspace is mounted when the run's original path is unknown.
const FALLBACK_WORKSPACE: &str = "/workspace";

pub fn run(dir: Option<String>, image: Option<String>, build_only: bool) {
  let dir = match resolve_dir(dir) {
    Ok(dir) => dir,
    Err(e) => {
      eprintln!("{e}");
      std::process::exit(1);
    }
  };

  // Refuse anything that isn't a rewindr artifact rather than producing a
  // broken environment from a half-matching directory.
  for file in REQUIRED_FILES {
    if !dir.join(file).exists() {
      eprintln!("{} is not a rewindr artifact (missing {file}).", dir.display());
      eprintln!("Expected files: {}", REQUIRED_FILES.join(", "));
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

  print_context(&dir);

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

  let image = image.unwrap_or_else(|| infer_image(&env));

  // Mount the workspace at the run's *original* path so $GITHUB_WORKSPACE and
  // any scripts using absolute runner paths resolve correctly.
  let mount_path = workspace_path(&env);
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

  println!("▸ Launching {image} (workspace mounted at {mount_path}) ...\n");
  match Command::new("docker").args(docker_args).status() {
    Ok(status) if status.success() => {}
    Ok(_) => std::process::exit(1),
    Err(e) => {
      eprintln!("Failed to run docker: {e}");
      std::process::exit(1);
    }
  }
}

/// Resolve the artifact directory: the given path, or the most recently
/// modified subdirectory of `./rewindr-artifacts`.
fn resolve_dir(dir: Option<String>) -> Result<PathBuf, String> {
  if let Some(dir) = dir {
    let path = PathBuf::from(dir);
    if !path.is_dir() {
      return Err(format!("No such directory: {}", path.display()));
    }
    return Ok(path);
  }

  let base = Path::new(ARTIFACTS_DIR);
  let entries = fs::read_dir(base).map_err(|_| {
    format!("No directory given and ./{ARTIFACTS_DIR} not found. Run `rewindr download <id>` first, or pass a path.")
  })?;

  entries
    .flatten()
    .filter(|e| e.path().is_dir())
    .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
    .max_by_key(|(mtime, _)| *mtime)
    .map(|(_, path)| path)
    .ok_or_else(|| format!("No artifacts found under ./{ARTIFACTS_DIR}. Run `rewindr download <id>` first."))
}

/// Best-effort — silently skips if the context file is absent or unparseable.
fn print_context(dir: &Path) {
  let Ok(raw) = fs::read_to_string(dir.join("github_context.json")) else {
    return;
  };
  let Ok(ctx) = serde_json::from_str::<serde_json::Value>(&raw) else {
    return;
  };
  let field = |key: &str| ctx.get(key).and_then(|v| v.as_str()).unwrap_or("?").to_string();
  println!(
    "▸ Replaying {} @ {} ({} / {})",
    field("repository"),
    &field("sha")[..field("sha").len().min(7)],
    field("workflow"),
    field("job"),
  );
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

/// Pick a base image from the runner's `ImageOS`.
///
/// Vanilla `ubuntu:*` images lack all of the tools GitHub preinstalls, so
/// we use the catthehacker runner mirrors (the same images `act` uses) for a
/// faithful rebuild. `--image` overrides this.
fn infer_image(env: &[(String, String)]) -> String {
  let image_os = env.iter().find(|(key, _)| key == "ImageOS").map(|(_, v)| v.as_str());
  match image_os {
    Some("ubuntu24") => "catthehacker/ubuntu:full-24.04",
    Some("ubuntu20") => "catthehacker/ubuntu:full-20.04",
    _ => "catthehacker/ubuntu:full-22.04",
  }
  .to_string()
}

/// The path to mount the workspace at inside the container: the run's original
/// `GITHUB_WORKSPACE`, so absolute runner paths keep resolving.
fn workspace_path(env: &[(String, String)]) -> String {
  env
    .iter()
    .find(|(key, _)| key == "GITHUB_WORKSPACE")
    .map(|(_, value)| value.clone())
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
