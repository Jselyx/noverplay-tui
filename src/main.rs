use anyhow::Result;
use clap::{Parser, Subcommand};
use noverplay_tui::{
    app::{App, Modal, Screen},
    config::{AppConfig, AppPaths},
    event::EventPump,
    storage::Storage,
    terminal::TerminalGuard,
    ui,
};

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
            run_tui().await?;
        }
    }
    Ok(())
}

async fn run_tui() -> Result<()> {
    let paths = AppPaths::discover()?;
    paths.ensure()?;
    let mut config = AppConfig::load(&paths)?.normalized();
    let storage = Storage::new(paths.database_file.clone());
    storage.initialize()?;
    let mut app = App::load(&storage, &config)?;
    let mut terminal = TerminalGuard::enter()?;
    let mut events = EventPump::new();

    while !app.should_quit {
        if app.dirty {
            terminal
                .terminal_mut()
                .draw(|frame| ui::draw(frame, &app))?;
            app.dirty = false;
        }
        let search_mode = app.screen == Screen::Search && app.modal.is_none();
        let onboarding_open = app.modal == Some(Modal::Onboarding);
        let action = events.next(search_mode, app.modal.is_some()).await;
        app.handle(action);
        if onboarding_open && app.modal.is_none() {
            config.onboarding_completed = true;
            config.guest_mode = true;
            app.config_dirty = true;
        }
        if app.queue_dirty {
            storage.save_queue(&app.queue_snapshot())?;
            app.queue_dirty = false;
        }
        if app.config_dirty {
            config.volume_percent = app.player.volume_percent;
            config.save(&paths)?;
            app.config_dirty = false;
        }
    }
    Ok(())
}
