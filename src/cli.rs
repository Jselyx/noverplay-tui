use std::{
    io::{self, Write},
    path::PathBuf,
};

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};

use crate::onboarding::zapret::{ZapretInstall, ZapretPlan, apply_plan};

#[derive(Debug, Parser)]
#[command(
    name = "noverplay",
    bin_name = "noverplay",
    version,
    about = "Noverplay в терминале"
)]
pub struct Cli {
    #[arg(long, hide = true)]
    pub background_player: bool,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    SetupZapret(SetupZapretArgs),
}

#[derive(Debug, Args)]
pub struct SetupZapretArgs {
    #[arg(long)]
    pub path: PathBuf,
    #[arg(long)]
    pub yes: bool,
}

pub fn run_command(command: Command) -> Result<()> {
    match command {
        Command::SetupZapret(args) => setup_zapret(args),
    }
}

fn setup_zapret(args: SetupZapretArgs) -> Result<()> {
    let install = ZapretInstall::detect(&args.path)?;
    let plan = ZapretPlan::build(install)?;
    println!("{}", plan.render_diff());
    if !plan.has_changes() {
        println!("SoundCloud уже есть в пользовательском списке, писать нечего");
        return Ok(());
    }
    if !args.yes && !ask_confirmation()? {
        println!("Изменения отменены");
        return Ok(());
    }
    let result = apply_plan(&plan)?;
    println!("Добавлено доменов: {}", result.added.len());
    if let Some(path) = result.backup_path {
        println!("Резервная копия: {}", path.display());
    }
    println!("Перезапусти Zapret вручную и снова проверь SoundCloud");
    Ok(())
}

fn ask_confirmation() -> Result<bool> {
    print!("Применить изменения? [y/N] ");
    io::stdout().flush().context("Не удалось показать вопрос")?;
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .context("Не удалось прочитать ответ")?;
    Ok(is_confirmation(&answer))
}

fn is_confirmation(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "y" | "yes" | "д" | "да"
    )
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn setup_command_keeps_the_binary_name_short() {
        let cli = Cli::try_parse_from([
            "noverplay",
            "setup-zapret",
            "--path",
            "/opt/zapret",
            "--yes",
        ])
        .unwrap();

        assert!(matches!(
            cli.command,
            Some(Command::SetupZapret(SetupZapretArgs { yes: true, .. }))
        ));
    }

    #[test]
    fn confirmation_is_explicit() {
        assert!(is_confirmation("да\n"));
        assert!(is_confirmation("Y"));
        assert!(!is_confirmation("ну наверное"));
        assert!(!is_confirmation(""));
    }

    #[test]
    fn windows_suffix_does_not_leak_into_help() {
        let help = Cli::command().render_help().to_string();

        assert!(help.contains("Usage: noverplay [COMMAND]"));
        assert!(!help.contains("noverplay.exe"));
    }
}
