use anyhow::Result;
use clap::Parser;
use noverplay_tui::{
    app::{App, Screen},
    audio::AudioEngine,
    cli::{Cli, run_command},
    config::{AppConfig, AppPaths},
    event::EventPump,
    hotkeys::GlobalHotkeys,
    runtime::Runtime,
    secrets::SecretStore,
    storage::Storage,
    terminal::TerminalGuard,
    ui,
};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(command) => run_command(command)?,
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
    let secrets = SecretStore::new(paths.secrets_file.clone());
    if let Some(client_id) = config.soundcloud_client_id_override.take() {
        secrets.set(
            noverplay_tui::secrets::SecretKey::SoundCloudClientIdOverride,
            &client_id,
        )?;
        config.save(&paths)?;
    }
    let storage = Storage::new(paths.database_file.clone());
    storage.initialize()?;
    let audio_outputs = if config.onboarding_completed {
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
    let mut terminal = TerminalGuard::enter()?;
    let mut events = EventPump::with_frame_limit(config.frame_limit);
    let hotkeys = match GlobalHotkeys::new(&config) {
        Ok(value) => value,
        Err(error) => {
            app.status_message = format!("Глобальные хоткеи выключены: {error}");
            None
        }
    };

    while !app.should_quit {
        if let Some(action) = hotkeys.as_ref().and_then(GlobalHotkeys::try_action) {
            app.handle(action);
        }
        drive_runtime(&mut app, &mut runtime);
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
        drive_runtime(&mut app, &mut runtime);
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
            config.save(&paths)?;
            app.config_dirty = false;
        }
    }
    Ok(())
}

fn drive_runtime(app: &mut App, runtime: &mut Runtime) {
    loop {
        let mut actions = runtime.poll_actions();
        let effects = app.take_effects();
        if actions.is_empty() && effects.is_empty() {
            break;
        }
        actions.extend(runtime.dispatch(effects));
        for action in actions {
            app.handle(action);
        }
    }
}
