use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Environment variable that overrides the stored token when set.
const TOKEN_ENV: &str = "REWINDR_GITHUB_TOKEN";

/// Persisted CLI configuration, stored as JSON in the user's config dir.
#[derive(Default, Serialize, Deserialize)]
pub struct Config {
    pub github_token: Option<String>,
}

/// Path to the config file: `<config_dir>/rewindr/config.json`.
fn config_path() -> Result<PathBuf, String> {
    let dir = dirs::config_dir().ok_or("could not determine the OS config directory")?;
    Ok(dir.join("rewindr").join("config.json"))
}

/// Load the stored config, returning a default (empty) config if none exists.
pub fn load() -> Result<Config, String> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(Config::default());
    }
    let contents = fs::read_to_string(&path).map_err(|e| format!("reading {path:?}: {e}"))?;
    serde_json::from_str(&contents).map_err(|e| format!("parsing {path:?}: {e}"))
}

/// Persist the config to disk with owner-only (0600) permissions.
pub fn save(config: &Config) -> Result<(), String> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("creating {parent:?}: {e}"))?;
    }
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| format!("writing {path:?}: {e}"))?;
    restrict_permissions(&path)?;
    Ok(())
}

#[cfg(unix)]
fn restrict_permissions(path: &PathBuf) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let perms = fs::Permissions::from_mode(0o600);
    fs::set_permissions(path, perms).map_err(|e| format!("setting permissions on {path:?}: {e}"))
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &PathBuf) -> Result<(), String> {
    Ok(())
}

/// Resolve the active token: the `REWINDR_GITHUB_TOKEN` env var takes
/// precedence, otherwise fall back to the stored config.
pub fn token() -> Result<Option<String>, String> {
    if let Ok(token) = std::env::var(TOKEN_ENV)
        && !token.is_empty()
    {
        return Ok(Some(token));
    }
    Ok(load()?.github_token)
}

pub fn fetch_user(token: &str) -> Result<crate::github::User, String> {
    crate::github::Client::new(token.to_string()).get("/user", &[])
}

/// Exits with guidance if no stored token is found.
pub fn require_token() -> String {
    match token() {
        Ok(Some(token)) => token,
        Ok(None) => {
            eprintln!("Not authenticated. Run `rewindr login` first.");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("Failed to read stored token: {e}");
            std::process::exit(1);
        }
    }
}
