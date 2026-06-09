use crate::auth;

pub fn run(limit: Option<i32>, workflow: Option<String>) {
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

  match auth::fetch_user(&token) {
    Ok(user) => println!("Authenticated as {}.", user.login),
    Err(e) => {
      eprintln!("Authentication check failed: {e}");
      eprintln!("Your token may be invalid or expired. Run `rewindr login` again.");
      std::process::exit(1);
    }
  }

  println!("List called {:?} {:?}", limit, workflow);
}
