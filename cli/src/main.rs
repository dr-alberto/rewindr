use clap::{Parser, Subcommand, Args};
mod commands;
mod artifacts;
mod auth;
mod github;

/// rewindr CLI
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Args)]
struct ListArgs {
  #[arg(short, long, default_value_t = 20, help="Maximum number of runs to show")]
  limit: u32,

  #[arg(short, long, help="Filter runs by workflow file (e.g. ci.yml)")]
  workflow: Option<String>,

  #[arg(short, long, help="Repository as owner/repo (auto-detected from git if omitted)")]
  repo: Option<String>
}

#[derive(Args)]
struct DownloadArgs {
  #[arg(help="Workflow run ID, or 'latest'")]
  run_id: String,

  #[arg(short, long, help="Repository as owner/repo (auto-detected from git if omitted)")]
  repo: Option<String>
}


#[derive(Args)]
struct PlayArgs {
  #[arg(help="Workflow run ID, or 'latest' (default: latest)")]
  run_id: Option<String>,

  #[arg(short, long, help="Repository as owner/repo (auto-detected from git if omitted)")]
  repo: Option<String>,

  #[arg(short, long, help="Base Docker image (default: inferred from the runner image)")]
  image: Option<String>,

  #[arg(long, help="Prepare the environment and print the docker command without entering it")]
  build_only: bool,

  #[arg(long, help="Play an already-extracted artifact directory instead of a run")]
  dir: Option<String>
}


#[derive(Subcommand)]
enum Command {
  /// List available items
  List(ListArgs),

  /// Download an environment
  Download(DownloadArgs),

  /// Rebuild and enter a captured environment in Docker
  Play(PlayArgs),

  /// Authenticate with GitHub and store a token
  Login
}


fn main() {
    let command = Cli::parse();

    match command.command{
      Command::List(args)=>commands::list::run(args.limit, args.workflow, args.repo),
      Command::Download(args)=>commands::download::run(args.run_id, args.repo),
      Command::Play(args)=>commands::play::run(args.run_id, args.repo, args.image, args.build_only, args.dir),
      Command::Login=>commands::login::run(),
    }
}