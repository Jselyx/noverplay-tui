use std::{
    net::TcpStream,
    time::{Duration, Instant},
};

use anyhow::Result;
use clap::Parser;
use noverplay_tui::{
    action::Action,
    app::{App, Screen},
    audio::AudioEngine,
    cli::{Cli, run_command},
    config::{AppConfig, AppPaths},
    control::{
        ControlCommand, ControlOwner, ControlRequest, ControlResponse, ControlServer, ResponseData,
        active_control_owner, send_command, send_response,
    },
    event::EventPump,
    hotkeys::GlobalHotkeys,
    runtime::Runtime,
    secrets::SecretStore,
    storage::Storage,
    terminal::TerminalGuard,
    ui,
};

type PendingControl = (TcpStream, ControlCommand, Instant);
const CONTROL_COMMAND_TIMEOUT: Duration = Duration::from_secs(28);

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.background_player {
        run_background_player().await?;
    } else {
        match cli.command {
            Some(command) => run_command(command)?,
            None => run_tui().await?,
        }
    }
    Ok(())
}

async fn run_tui() -> Result<()> {
    let paths = AppPaths::discover()?;
    paths.ensure()?;
    stop_background_player(&paths).await?;
    let control = ControlServer::bind(&paths)?;
    let (mut config, storage, mut app, mut runtime) = load_player(&paths, true)?;
    let mut terminal = TerminalGuard::enter()?;
    let mut events = EventPump::with_frame_limit(config.frame_limit);
    let hotkeys = match GlobalHotkeys::new(&config) {
        Ok(value) => value,
        Err(error) => {
            app.status_message = format!("Глобальные хоткеи выключены: {error}");
            None
        }
    };
    let mut pending_control = None;

    while !app.should_quit {
        poll_control(&control, &mut app, &mut pending_control);
        if let Some(action) = hotkeys.as_ref().and_then(GlobalHotkeys::try_action) {
            app.handle(action);
        }
        drive_runtime(&mut app, &mut runtime, &mut pending_control);
        app.show_keybindings_notice_if_needed();
        if app.dirty {
            terminal
                .terminal_mut()
                .draw(|frame| ui::draw(frame, &app))?;
            app.dirty = false;
        }
        let search_mode = app.screen == Screen::Search && app.modal.is_none();
        let search_has_results = search_mode && !app.search_results.is_empty();
        let onboarding_open = app.onboarding_open();
        let action = events
            .next(
                search_mode,
                app.search_input_focused,
                app.modal.is_some(),
                app.text_modal_open(),
                search_has_results,
            )
            .await;
        app.handle(action);
        drive_runtime(&mut app, &mut runtime, &mut pending_control);
        if onboarding_open && !app.onboarding_open() {
            config.onboarding_completed = true;
            if let Some(result) = app.take_onboarding_result() {
                config.guest_mode = matches!(
                    result.account_mode,
                    noverplay_tui::onboarding::AccountMode::Guest
                );
                config.soundcloud_enabled = result.soundcloud_enabled;
                config.yandex_enabled = result.yandex_enabled;
                config.audio_output = result.audio_output;
            }
            app.config_dirty = true;
        }
        persist_player(&paths, &storage, &mut config, &mut app)?;
    }
    Ok(())
}

async fn run_background_player() -> Result<()> {
    let paths = AppPaths::discover()?;
    paths.ensure()?;
    let control = ControlServer::bind_background(&paths)?;
    let (mut config, storage, mut app, mut runtime) = load_player(&paths, false)?;
    let hotkeys = GlobalHotkeys::new(&config).unwrap_or_default();
    let mut pending_control = None;

    while !app.should_quit {
        poll_control(&control, &mut app, &mut pending_control);
        if let Some(action) = hotkeys.as_ref().and_then(GlobalHotkeys::try_action) {
            app.handle(action);
        }
        drive_runtime(&mut app, &mut runtime, &mut pending_control);
        persist_player(&paths, &storage, &mut config, &mut app)?;
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Ok(())
}

fn load_player(
    paths: &AppPaths,
    discover_audio_outputs: bool,
) -> Result<(AppConfig, Storage, App, Runtime)> {
    let mut config = AppConfig::load(paths)?.normalized();
    let secrets = SecretStore::new(paths.secrets_file.clone());
    if let Some(client_id) = config.soundcloud_client_id_override.take() {
        secrets.set(
            noverplay_tui::secrets::SecretKey::SoundCloudClientIdOverride,
            &client_id,
        )?;
        config.save(paths)?;
    }
    let storage = Storage::new(paths.database_file.clone());
    storage.initialize()?;
    let audio_outputs = if config.onboarding_completed || !discover_audio_outputs {
        Vec::new()
    } else {
        AudioEngine::output_devices().unwrap_or_default()
    };
    let mut app = App::load_with_audio_outputs(&storage, &config, audio_outputs)?;
    let mut runtime = Runtime::new(&config, &secrets, storage.clone());
    match runtime.credential_state() {
        Ok(credentials) => app.set_credentials(credentials),
        Err(error) => app.status_message = format!("Не удалось проверить ключи: {error}"),
    }
    if let Some(notice) = runtime.take_notices().into_iter().last() {
        app.status_message = notice;
    }
    app.restore_account();
    Ok((config, storage, app, runtime))
}

fn persist_player(
    paths: &AppPaths,
    storage: &Storage,
    config: &mut AppConfig,
    app: &mut App,
) -> Result<()> {
    if app.queue_dirty {
        storage.save_queue(&app.queue_snapshot())?;
        app.queue_dirty = false;
    }
    if app.config_dirty {
        config.volume_percent = app.player.volume_percent;
        config.soundcloud_enabled = app.soundcloud_enabled;
        config.yandex_enabled = app.yandex_enabled;
        config.deezer_enabled = app.deezer_enabled;
        config.global_hotkeys_enabled = app.global_hotkeys_enabled;
        config.hotkeys = app.hotkeys.clone();
        config.keybindings_notice_seen = app.keybindings_notice_seen;
        config.guest_mode = app.account.user().is_none();
        config.soundcloud_client_id_refresh_at_ms = app.soundcloud_refresh_at_ms;
        config.save(paths)?;
        app.config_dirty = false;
    }
    Ok(())
}

async fn stop_background_player(paths: &AppPaths) -> Result<()> {
    if active_control_owner(paths) != Some(ControlOwner::Background) {
        return Ok(());
    }
    let response = send_command(paths, ControlCommand::Shutdown)?;
    if !response.ok {
        anyhow::bail!(response.message);
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while paths.control_endpoint_file.exists() {
        if Instant::now() >= deadline {
            anyhow::bail!("Фоновый плеер не освободил control endpoint");
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    Ok(())
}

fn poll_control(control: &ControlServer, app: &mut App, pending: &mut Option<PendingControl>) {
    if pending
        .as_ref()
        .is_some_and(|(_, _, started)| started.elapsed() >= CONTROL_COMMAND_TIMEOUT)
        && let Some((mut stream, _, _)) = pending.take()
    {
        let _ = send_response(
            &mut stream,
            &ControlResponse::error("Control-команда превысила таймаут"),
        );
    }
    for (mut stream, request) in control.poll() {
        if is_async_control(&request.command) && pending.is_some() {
            let _ = send_response(
                &mut stream,
                &ControlResponse::error("Предыдущая поисковая команда ещё выполняется"),
            );
        } else if let Some(command) = begin_control(app, request, &mut stream) {
            *pending = Some((stream, command, Instant::now()));
        }
    }
}

fn is_async_control(command: &ControlCommand) -> bool {
    matches!(
        command,
        ControlCommand::Play { .. }
            | ControlCommand::Search { .. }
            | ControlCommand::QueueAdd { .. }
            | ControlCommand::Wave
    )
}

fn begin_control(
    app: &mut App,
    request: ControlRequest,
    stream: &mut TcpStream,
) -> Option<ControlCommand> {
    let command = request.command;
    let response: Result<ControlResponse, String> = match &command {
        ControlCommand::Play { query, provider }
        | ControlCommand::Search { query, provider }
        | ControlCommand::QueueAdd { query, provider } => {
            app.control_search(query.clone(), *provider);
            return Some(command);
        }
        ControlCommand::Wave => {
            app.control_wave();
            return Some(command);
        }
        ControlCommand::Pause => app
            .control_pause()
            .map(|_| ControlResponse::accepted("Пауза")),
        ControlCommand::Resume => app
            .control_resume()
            .map(|_| ControlResponse::accepted("Воспроизведение продолжено")),
        ControlCommand::Toggle => app
            .control_toggle()
            .map(|_| ControlResponse::accepted("Состояние переключено")),
        ControlCommand::Next => {
            if app.queue.is_empty() {
                Err("очередь пуста".to_string())
            } else {
                app.control_next();
                Ok(ControlResponse::accepted("Следующий трек"))
            }
        }
        ControlCommand::Previous => {
            if app.queue.is_empty() {
                Err("очередь пуста".to_string())
            } else {
                app.control_previous();
                Ok(ControlResponse::accepted("Предыдущий трек"))
            }
        }
        ControlCommand::Stop => {
            app.control_stop();
            Ok(ControlResponse::accepted("Остановлено"))
        }
        ControlCommand::QueueList => Ok(ControlResponse::with_data(
            "Очередь",
            ResponseData::Tracks(app.queue.clone()),
        )),
        ControlCommand::QueueRemove { index } => app.control_queue_remove(*index).map(|track| {
            ControlResponse::accepted(format!(
                "Удалено: {} — {}",
                track.display_artist(),
                track.title
            ))
        }),
        ControlCommand::QueueClear => {
            app.control_queue_clear();
            Ok(ControlResponse::accepted("Очередь очищена"))
        }
        ControlCommand::Status => Ok(ControlResponse::with_data(
            "Текущее состояние",
            ResponseData::Status(Box::new(app.status_snapshot())),
        )),
        ControlCommand::Shutdown => {
            app.should_quit = true;
            Ok(ControlResponse::accepted("Фоновый плеер остановлен"))
        }
    };
    let _ = send_response(stream, &response.unwrap_or_else(ControlResponse::error));
    None
}

fn drive_runtime(app: &mut App, runtime: &mut Runtime, pending: &mut Option<PendingControl>) {
    loop {
        let mut actions = runtime.poll_actions();
        let effects = app.take_effects();
        if actions.is_empty() && effects.is_empty() {
            break;
        }
        actions.extend(runtime.dispatch(effects));
        for action in actions {
            if !finish_control(app, &action, pending) {
                app.handle(action);
            }
        }
    }
}

fn finish_control(app: &mut App, action: &Action, pending: &mut Option<PendingControl>) -> bool {
    let (tracks, failures, wave_result) = match action {
        Action::ControlSearchFinished { tracks, failures } => (tracks, failures, false),
        Action::ControlWaveFinished { tracks, failures } => (tracks, failures, true),
        _ => return false,
    };
    let matches_pending = pending
        .as_ref()
        .is_some_and(|(_, command, _)| wave_result == matches!(command, ControlCommand::Wave));
    if !matches_pending {
        return true;
    }
    let Some((mut stream, command, _)) = pending.take() else {
        return true;
    };
    let playable = tracks
        .iter()
        .find(|track| track.capability.can_play())
        .cloned();
    let response = match command {
        ControlCommand::Search { .. } if tracks.is_empty() => ControlResponse::error(
            failures
                .first()
                .cloned()
                .unwrap_or_else(|| "Ничего не найдено".to_string()),
        ),
        ControlCommand::Search { .. } => ControlResponse::with_data(
            format!("Найдено: {}", tracks.len()),
            ResponseData::Tracks(tracks.clone()),
        ),
        ControlCommand::Play { .. } => match playable {
            Some(track) => {
                app.control_play(track.clone());
                ControlResponse::accepted(format!(
                    "Запускаю: {} — {}",
                    track.display_artist(),
                    track.title
                ))
            }
            None => ControlResponse::error(
                failures
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "Нет playable результатов".to_string()),
            ),
        },
        ControlCommand::Wave => {
            let playable = tracks
                .iter()
                .filter(|track| track.capability.can_play())
                .cloned()
                .collect::<Vec<_>>();
            match app.control_play_wave(playable) {
                Some((track, count)) => ControlResponse::accepted(format!(
                    "Запускаю Мою волну ({count}): {} — {}",
                    track.display_artist(),
                    track.title
                )),
                None => ControlResponse::error(
                    failures
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "Волна не нашла playable треков".to_string()),
                ),
            }
        }
        ControlCommand::QueueAdd { .. } => match playable {
            Some(track) => {
                let position = app.control_queue_add(track);
                ControlResponse::accepted(format!("Добавлено в очередь: {position}"))
            }
            None => ControlResponse::error(
                failures
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "Нет playable результатов".to_string()),
            ),
        },
        _ => ControlResponse::error("Неожиданное завершение control-команды"),
    };
    app.status_message = response.message.clone();
    let _ = send_response(&mut stream, &response);
    true
}

#[cfg(test)]
mod tests {
    use std::{io::BufRead, net::TcpListener, time::Duration};

    use noverplay_tui::model::{PlaybackCapability, ProviderKind, TrackRef};
    use url::Url;

    use super::*;

    #[test]
    fn async_control_sends_one_final_response() {
        let (_temp, mut app) = test_app();
        let (client, mut server) = stream_pair();
        client
            .set_read_timeout(Some(Duration::from_millis(50)))
            .unwrap();
        let command = ControlCommand::Play {
            query: "test".to_string(),
            provider: noverplay_tui::model::SearchProvider::All,
        };
        let pending_command = begin_control(
            &mut app,
            ControlRequest {
                token: "unused".to_string(),
                command,
            },
            &mut server,
        )
        .unwrap();
        let mut byte = [0_u8; 1];
        assert!(
            client.peek(&mut byte).is_err(),
            "preliminary response leaked"
        );

        let mut pending = Some((server, pending_command, Instant::now()));
        assert!(finish_control(
            &mut app,
            &Action::ControlSearchFinished {
                tracks: vec![track("1")],
                failures: Vec::new(),
            },
            &mut pending,
        ));
        assert!(pending.is_none());
        client
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut line = String::new();
        std::io::BufReader::new(client)
            .read_line(&mut line)
            .unwrap();
        let response: ControlResponse = serde_json::from_str(&line).unwrap();
        assert!(response.ok);
        assert!(response.data.is_none());
        assert!(response.message.contains("Track 1"));
        assert_eq!(
            app.now_playing.as_ref().map(|track| track.id.as_str()),
            Some("1")
        );
    }

    #[test]
    fn stale_wave_result_does_not_finish_a_pending_search() {
        let (_temp, mut app) = test_app();
        let (_client, server) = stream_pair();
        let mut pending = Some((
            server,
            ControlCommand::Search {
                query: "test".to_string(),
                provider: noverplay_tui::model::SearchProvider::All,
            },
            Instant::now(),
        ));
        assert!(finish_control(
            &mut app,
            &Action::ControlWaveFinished {
                tracks: vec![track("stale")],
                failures: Vec::new(),
            },
            &mut pending,
        ));
        assert!(pending.is_some());
        assert!(app.now_playing.is_none());
    }

    fn test_app() -> (tempfile::TempDir, App) {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path().join("library.sqlite3"));
        storage.initialize().unwrap();
        let app = App::load(&storage, &AppConfig::default()).unwrap();
        (temp, app)
    }

    fn stream_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        (client, server)
    }

    fn track(id: &str) -> TrackRef {
        TrackRef {
            provider: ProviderKind::SoundCloud,
            id: id.to_string(),
            title: format!("Track {id}"),
            artists: vec!["Artist".to_string()],
            duration_ms: Some(120_000),
            artwork_url: None,
            web_url: Url::parse("https://soundcloud.com/test/track").unwrap(),
            capability: PlaybackCapability::Full,
            genres: Vec::new(),
            explicit: false,
            drm: false,
        }
    }
}
