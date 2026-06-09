use clap::{Parser, Subcommand, Args};
mod commands;
mod auth;

/// rewindr CLI
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Args)]
struct ListArgs {
  #[arg(short, long, help="Maximum number of results")]
  limit: Option<i32>,

  #[arg(short, long, help="Filter results by workflow")]
  workflow: Option<String>
}

#[derive(Args)]
struct DownloadArgs {
  #[arg(short, long, help="ID of the item to download")]
  id: Option<String>,

  #[arg(short, long, help="Output file path")]
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
      Command::List(args)=>commands::list::run(args.limit, args.workflow),
      Command::Download(args)=>commands::download::run(args.id, args.out),
      Command::Play=>commands::play::run(),
      Command::Login=>commands::login::run(),
    }
}