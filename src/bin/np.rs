use std::{
    env,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use chrono::{Local, LocalResult, NaiveDate, TimeZone};
use clap::Parser;
use noverplay_tui::{
    config::AppPaths,
    control::{
        ControlCommand, HistoryCommand, NpCli, NpCommand, QueueCommand, ResponseData,
        active_control_owner, send_command, split_provider_tag,
    },
    storage::{HistoryEntry, Storage},
};

fn main() -> Result<()> {
    let paths = AppPaths::discover()?;
    let cli = NpCli::parse();
    match cli.command {
        NpCommand::History(args) => run_history(&paths, args.command),
        command => run_control(&paths, command),
    }
}

fn run_control(paths: &AppPaths, command: NpCommand) -> Result<()> {
    let (command, json) = match command {
        NpCommand::Play(args) => {
            let (query, provider) = split_provider_tag(&args.query, args.provider)?;
            (ControlCommand::Play { query, provider }, false)
        }
        NpCommand::Search(args) => {
            let (query, provider) = split_provider_tag(&args.query, args.provider)?;
            (ControlCommand::Search { query, provider }, false)
        }
        NpCommand::Wave => (ControlCommand::Wave, false),
        NpCommand::Pause => (ControlCommand::Pause, false),
        NpCommand::Resume => (ControlCommand::Resume, false),
        NpCommand::Toggle => (ControlCommand::Toggle, false),
        NpCommand::Next => (ControlCommand::Next, false),
        NpCommand::Previous => (ControlCommand::Previous, false),
        NpCommand::Stop => (ControlCommand::Stop, false),
        NpCommand::Status(args) => (ControlCommand::Status, args.json),
        NpCommand::Queue(args) => match args.command {
            QueueCommand::List => (ControlCommand::QueueList, false),
            QueueCommand::Add(args) => {
                let (query, provider) = split_provider_tag(&args.query, args.provider)?;
                (ControlCommand::QueueAdd { query, provider }, false)
            }
            QueueCommand::Remove { index } => (ControlCommand::QueueRemove { index }, false),
            QueueCommand::Clear => (ControlCommand::QueueClear, false),
        },
        NpCommand::History(_) => unreachable!(),
    };
    let response = send_command_with_autostart(paths, command)?;
    if json {
        match response.data.as_ref() {
            Some(ResponseData::Status(status)) if response.ok => {
                println!("{}", serde_json::to_string(status)?);
            }
            _ => println!("{}", serde_json::to_string(&response)?),
        }
    } else if let Some(ResponseData::Tracks(tracks)) = response.data.as_ref() {
        print_tracks(tracks);
    } else if let Some(ResponseData::Status(status)) = response.data.as_ref() {
        println!(
            "{} · {}/{} ms · volume {}%",
            status.playback, status.position_ms, status.duration_ms, status.volume_percent
        );
        if let Some(track) = status.track.as_ref() {
            println!("{} — {}", track.display_artist(), track.title);
        }
        println!(
            "queue: {}/{}",
            status.queue_index.map(|value| value + 1).unwrap_or(0),
            status.queue_length
        );
    } else {
        println!("{}", response.message);
    }
    if !response.ok {
        bail!("команда отклонена");
    }
    Ok(())
}

fn send_command_with_autostart(
    paths: &AppPaths,
    command: ControlCommand,
) -> Result<noverplay_tui::control::ControlResponse> {
    let first_error = match send_command(paths, command.clone()) {
        Ok(response) => return Ok(response),
        Err(error) => error,
    };
    if active_control_owner(paths).is_some() {
        return Err(first_error);
    }

    let mut child = spawn_background_player()?;
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        match send_command(paths, command.clone()) {
            Ok(response) => return Ok(response),
            Err(error) if Instant::now() >= deadline => {
                return Err(error).context("Фоновый плеер не успел запуститься");
            }
            Err(_) => {}
        }
        if let Some(status) = child.try_wait()? {
            bail!("Фоновый плеер завершился при запуске: {status}");
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn spawn_background_player() -> Result<Child> {
    let binary = background_player_binary()?;
    let mut command = Command::new(&binary);
    command
        .arg("--background-player")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        // консоль ушла спать, музыка решила что ей вообще-то необязательно
        command.creation_flags(0x0800_0000);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        // SAFETY: pre_exec будит только async-signal-safe setsid, остальной Rust продолжает спать
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
    }
    command
        .spawn()
        .with_context(|| format!("Не удалось запустить {}", binary.display()))
}

fn background_player_binary() -> Result<PathBuf> {
    background_player_binary_next_to(&env::current_exe()?)
}

fn background_player_binary_next_to(current_exe: &Path) -> Result<PathBuf> {
    let file_name = if cfg!(windows) {
        "noverplay.exe"
    } else {
        "noverplay"
    };
    let sibling = current_exe
        .parent()
        .context("np остался без каталога бинарника")?
        .join(file_name);
    if sibling.is_file() {
        Ok(sibling)
    } else {
        Ok(PathBuf::from(file_name))
    }
}

fn run_history(paths: &AppPaths, command: HistoryCommand) -> Result<()> {
    let storage = Storage::new(paths.database_file.clone());
    storage.initialize()?;
    let (entries, json) = match command {
        HistoryCommand::Recent(args) => (storage.recent_history(args.limit)?, args.json),
        HistoryCommand::Today(args) => {
            let (start_ms, end_ms) = local_today_bounds_ms()?;
            (
                storage.history_between(start_ms, end_ms, 10_000)?,
                args.json,
            )
        }
    };
    if json {
        println!("{}", serde_json::to_string(&entries)?);
    } else {
        print_history(&entries);
    }
    Ok(())
}

fn print_tracks(tracks: &[noverplay_tui::model::TrackRef]) {
    if tracks.is_empty() {
        println!("Пусто");
        return;
    }
    for (index, track) in tracks.iter().enumerate() {
        println!(
            "{}: {} — {} [{}]",
            index + 1,
            track.display_artist(),
            track.title,
            track.provider.label()
        );
    }
}

fn print_history(entries: &[HistoryEntry]) {
    if entries.is_empty() {
        println!("История пуста");
        return;
    }
    for entry in entries {
        let played_at = Local
            .timestamp_millis_opt(entry.played_at_ms)
            .single()
            .map(|value| value.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| entry.played_at_ms.to_string());
        println!(
            "{}  {} — {} [{}]",
            played_at,
            entry.track.display_artist(),
            entry.track.title,
            entry.track.provider.label()
        );
    }
}

fn local_today_bounds_ms() -> Result<(i64, i64)> {
    let today = Local::now().date_naive();
    let tomorrow = today
        .succ_opt()
        .ok_or_else(|| anyhow::anyhow!("Не удалось вычислить следующий локальный день"))?;
    Ok((local_day_start(today)?, local_day_start(tomorrow)?))
}

fn local_day_start(date: NaiveDate) -> Result<i64> {
    for minute in 0..=180_u32 {
        let hour = minute / 60;
        let minute = minute % 60;
        let local = date
            .and_hms_opt(hour, minute, 0)
            .ok_or_else(|| anyhow::anyhow!("Некорректная граница локального дня"))?;
        match Local.from_local_datetime(&local) {
            LocalResult::Single(value) => return Ok(value.timestamp_millis()),
            LocalResult::Ambiguous(first, second) => {
                return Ok(first.min(second).timestamp_millis());
            }
            LocalResult::None => {}
        }
    }
    bail!("Не удалось определить начало локального дня {date}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_today_bounds_contain_now_and_allow_dst_length() {
        let (start, end) = local_today_bounds_ms().unwrap();
        let now = Local::now().timestamp_millis();
        assert!(start <= now && now < end);
        assert!((22 * 3_600_000..=26 * 3_600_000).contains(&(end - start)));
    }

    #[test]
    fn background_player_is_resolved_next_to_np() {
        let temp = tempfile::tempdir().unwrap();
        let np = temp
            .path()
            .join(if cfg!(windows) { "np.exe" } else { "np" });
        let expected = temp.path().join(if cfg!(windows) {
            "noverplay.exe"
        } else {
            "noverplay"
        });
        std::fs::write(&expected, []).unwrap();

        assert_eq!(background_player_binary_next_to(&np).unwrap(), expected);
    }
}
