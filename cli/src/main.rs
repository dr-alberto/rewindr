use clap::{Parser, Subcommand, Args};
mod commands;
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
  #[arg(help="Workflow run ID to download the rewindr artifact from")]
  run_id: u64,

  #[arg(short, long, help="Repository as owner/repo (auto-detected from git if omitted)")]
  repo: Option<String>,

  #[arg(short, long, help="Output directory (default: rewindr-artifacts/<run-id>)")]
  out: Option<String>
}


#[derive(Subcommand)]
enum Command {
  /// List available items
  List(ListArgs),

  /// Download an environment
  Download(DownloadArgs),
  
  /// Setup the environment locally
  Play,

  /// Authenticate with GitHub and store a token
  Login
}


fn main() {
    let command = Cli::parse();

    match command.command{
      Command::List(args)=>commands::list::run(args.limit, args.workflow, args.repo),
      Command::Download(args)=>commands::download::run(args.run_id, args.repo, args.out),
      Command::Play=>commands::play::run(),
      Command::Login=>commands::login::run(),
    }
}