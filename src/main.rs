use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "noverplay", version, about = "Noverplay в терминале")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    SetupZapret {
        #[arg(long)]
        path: std::path::PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::SetupZapret { path }) => {
            println!("Путь к Zapret: {}", path.display());
        }
        None => {
            println!("{} {}", noverplay_tui::APP_NAME, noverplay_tui::APP_VERSION);
        }
    }
    Ok(())
}
