use std::process::Command;

use serde::Deserialize;
use serde::de::DeserializeOwned;

const API_BASE: &str = "https://api.github.com";
const USER_AGENT: &str = "rewindr-cli";

/// Only artifacts whose name starts with this prefix are rewindr environments.
pub const ARTIFACT_PREFIX: &str = "rewindr";

/// An authenticated GitHub REST client.
pub struct Client {
    http: reqwest::blocking::Client,
    token: String,
}

impl Client {
    pub fn new(token: String) -> Self {
        Self {
            http: reqwest::blocking::Client::new(),
            token,
        }
    }

    /// Build an authenticated GET request with the common GitHub headers.
    fn request(&self, path: &str) -> reqwest::blocking::RequestBuilder {
        self.http
            .get(format!("{API_BASE}{path}"))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", USER_AGENT)
    }

    pub fn get<T: DeserializeOwned>(&self, path: &str, params: &[(&str, &str)]) -> Result<T, String> {
        let response = self
            .request(path)
            .query(params)
            .send()
            .map_err(network_error)?;

        if !response.status().is_success() {
            return Err(explain_status(response.status()));
        }

        response
            .json::<T>()
            .map_err(|e| format!("decoding response: {e}"))
    }

    /// GitHub responds with a redirect to a zip; the blocking client follows it automatically.
    pub fn get_bytes(&self, path: &str) -> Result<Vec<u8>, String> {
        let response = self.request(path).send().map_err(network_error)?;

        if !response.status().is_success() {
            return Err(explain_status(response.status()));
        }

        response
            .bytes()
            .map(|b| b.to_vec())
            .map_err(|e| format!("reading response body: {e}"))
    }
}

fn network_error(e: reqwest::Error) -> String {
    if e.is_timeout() {
        "request to GitHub timed out".to_string()
    } else if e.is_connect() {
        "could not reach GitHub: check your network connection".to_string()
    } else {
        format!("request to GitHub failed: {e}")
    }
}

fn explain_status(status: reqwest::StatusCode) -> String {
    use reqwest::StatusCode;
    match status {
        StatusCode::UNAUTHORIZED => {
            "authentication failed: your token may be invalid or expired; run `rewindr login`".to_string()
        }
        StatusCode::FORBIDDEN => {
            "access denied: the token may lack permissions, or you've hit the GitHub API rate limit".to_string()
        }
        StatusCode::NOT_FOUND => {
            "not found: check the repository name and run id, and that your token can read it".to_string()
        }
        StatusCode::GONE => {
            "the artifact has expired: GitHub deletes run artifacts after their retention period".to_string()
        }
        s if s.is_server_error() => format!("GitHub had a server error ({s}); try again shortly"),
        s => format!("GitHub returned {s}"),
    }
}

/// A GitHub user, as returned by `GET /user`.
#[derive(Deserialize)]
pub struct User {
    pub login: String,
}

/// A page of workflow runs.
#[derive(Deserialize)]
pub struct WorkflowRuns {
    pub workflow_runs: Vec<WorkflowRun>,
}

#[derive(Deserialize)]
pub struct WorkflowRun {
    pub id: u64,
    #[serde(default)]
    pub status: String,
    pub conclusion: Option<String>,
    pub head_branch: Option<String>,
    pub created_at: Option<String>,
}

/// A page of artifacts for a workflow run.
#[derive(Deserialize)]
pub struct Artifacts {
    pub artifacts: Vec<Artifact>,
}

#[derive(Deserialize)]
pub struct Artifact {
    pub id: u64,
    pub name: String,
    pub workflow_run: Option<ArtifactRun>,
}

/// The run an artifact belongs to (embedded in the repo artifacts listing).
#[derive(Deserialize)]
pub struct ArtifactRun {
    pub id: u64,
}

/// Resolve `owner/repo` from the argument or the git remote, or exit.
pub fn require_repo(repo: Option<String>) -> String {
    match repo.or_else(detect_repo) {
        Some(repo) => repo,
        None => {
            eprintln!("Could not detect repository. Pass --repo owner/repo.");
            std::process::exit(1);
        }
    }
}

/// The most recent run that has a rewindr artifact. Artifacts come back
/// newest-first, so the first matching one points at the latest such run.
pub fn latest_run_with_artifact(client: &Client, repo: &str) -> Result<u64, String> {
    let artifacts: Artifacts =
        client.get(&format!("/repos/{repo}/actions/artifacts"), &[("per_page", "100")])?;
    artifacts
        .artifacts
        .iter()
        .filter(|a| a.name.starts_with(ARTIFACT_PREFIX))
        .find_map(|a| a.workflow_run.as_ref().map(|run| run.id))
        .ok_or_else(|| format!("no rewindr artifacts found for {repo}"))
}

pub fn detect_repo() -> Option<String> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let url = String::from_utf8(output.stdout).ok()?;
    parse_repo(url.trim())
}

fn parse_repo(url: &str) -> Option<String> {
    let path = url
        .strip_prefix("git@github.com:")
        .or_else(|| url.strip_prefix("https://github.com/"))
        .or_else(|| url.strip_prefix("ssh://git@github.com/"))?;
    let path = path.strip_suffix(".git").unwrap_or(path);
    match path.matches('/').count() {
        1 if !path.starts_with('/') && !path.ends_with('/') => Some(path.to_string()),
        _ => None,
    }
}
