use crate::auth::{self, Config};

pub fn run() {
    println!("Generate a GitHub personal access token at:");
    println!("  https://github.com/settings/tokens");
    println!("It needs read access to Actions:");
    println!("  - fine-grained: the \"Actions\" repository permission (read-only)");
    println!("  - classic: the \"repo\" scope (\"public_repo\" if you only target public repos)");
    println!();

    let token = match rpassword::prompt_password("Paste your token: ") {
        Ok(t) => t.trim().to_string(),
        Err(e) => {
            eprintln!("Failed to read token: {e}");
            std::process::exit(1);
        }
    };

    if token.is_empty() {
        eprintln!("No token provided.");
        std::process::exit(1);
    }

    let user = match auth::fetch_user(&token) {
        Ok(user) => user,
        Err(e) => {
            eprintln!("Token validation failed: {e}");
            std::process::exit(1);
        }
    };

    let config = Config {
        github_token: Some(token),
    };
    if let Err(e) = auth::save(&config) {
        eprintln!("Failed to save token: {e}");
        std::process::exit(1);
    }

    println!("Logged in as {}.", user.login);
}
