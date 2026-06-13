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

    /// Perform an authenticated GET and decode the JSON response.
    pub fn get<T: DeserializeOwned>(&self, path: &str, params: &[(&str, &str)]) -> Result<T, String> {
        let response = self
            .request(path)
            .query(params)
            .send()
            .map_err(|e| format!("request failed: {e}"))?;

        if !response.status().is_success() {
            return Err(format!("GitHub returned {}", response.status()));
        }

        response
            .json::<T>()
            .map_err(|e| format!("decoding response: {e}"))
    }

    /// Perform an authenticated GET and return the raw response bytes.
    ///
    /// Used for artifact downloads, where GitHub responds with a redirect to a
    /// zip that the blocking client follows automatically.
    pub fn get_bytes(&self, path: &str) -> Result<Vec<u8>, String> {
        let response = self
            .request(path)
            .send()
            .map_err(|e| format!("request failed: {e}"))?;

        if !response.status().is_success() {
            return Err(format!("GitHub returned {}", response.status()));
        }

        response
            .bytes()
            .map(|b| b.to_vec())
            .map_err(|e| format!("reading response body: {e}"))
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

/// Detect the `owner/repo` slug from the `origin` git remote, if any.
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

/// Extract `owner/repo` from an HTTPS or SSH GitHub remote URL.
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
