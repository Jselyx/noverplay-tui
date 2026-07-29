use anyhow::Result;

use crate::{
    action::Action,
    config::AppConfig,
    credentials::{CredentialEditor, CredentialKind, CredentialState},
    effect::AppEffect,
    importer::PlaylistImportEditor,
    model::{Playlist, RepeatMode, TrackRef},
    onboarding::{AccountMode, OnboardingCommand, OnboardingResult, OnboardingState},
    storage::{QueueSnapshot, Storage},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Screen {
    #[default]
    Home,
    Wave,
    Search,
    Library,
    Playlists,
    Queue,
    Profile,
    Settings,
}

impl Screen {
    pub const ALL: [Self; 8] = [
        Self::Home,
        Self::Wave,
        Self::Search,
        Self::Library,
        Self::Playlists,
        Self::Queue,
        Self::Profile,
        Self::Settings,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Home => "Главная",
            Self::Wave => "Моя волна",
            Self::Search => "Поиск",
            Self::Library => "Библиотека",
            Self::Playlists => "Плейлисты",
            Self::Queue => "Очередь",
            Self::Profile => "Профиль",
            Self::Settings => "Настройки",
        }
    }

    pub const fn icon(self) -> &'static str {
        match self {
            Self::Home => "⌂",
            Self::Wave => "≈",
            Self::Search => "⌕",
            Self::Library => "♥",
            Self::Playlists => "≡",
            Self::Queue => "↯",
            Self::Profile => "●",
            Self::Settings => "⚙",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PlaybackStatus {
    Playing,
    #[default]
    Paused,
    Buffering,
    Stopped,
}

#[derive(Clone, Debug)]
pub struct PlayerState {
    pub status: PlaybackStatus,
    pub position_ms: u64,
    pub buffered_ms: u64,
    pub duration_ms: u64,
    pub volume_percent: u8,
    pub shuffle: bool,
    pub repeat: RepeatMode,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            status: PlaybackStatus::Stopped,
            position_ms: 0,
            buffered_ms: 0,
            duration_ms: 0,
            volume_percent: 75,
            shuffle: false,
            repeat: RepeatMode::Off,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Modal {
    Onboarding(Box<OnboardingState>),
    Credential(Box<CredentialEditor>),
    PlaylistImport(Box<PlaylistImportEditor>),
    Help,
    CommandPalette,
}

#[derive(Debug)]
pub struct App {
    pub screen: Screen,
    pub selected: usize,
    pub search_query: String,
    pub search_results: Vec<TrackRef>,
    pub wave_tracks: Vec<TrackRef>,
    pub wave_loading: bool,
    pub library: Vec<TrackRef>,
    pub playlists: Vec<Playlist>,
    pub queue: Vec<TrackRef>,
    pub queue_index: Option<usize>,
    pub now_playing: Option<TrackRef>,
    pub player: PlayerState,
    pub credentials: CredentialState,
    pub soundcloud_enabled: bool,
    pub yandex_enabled: bool,
    pub modal: Option<Modal>,
    pub should_quit: bool,
    pub dirty: bool,
    pub queue_dirty: bool,
    pub config_dirty: bool,
    pub status_message: String,
    onboarding_result: Option<OnboardingResult>,
    effects: Vec<AppEffect>,
}

impl App {
    pub fn load(storage: &Storage, config: &AppConfig) -> Result<Self> {
        Self::load_with_audio_outputs(storage, config, Vec::new())
    }

    pub fn load_with_audio_outputs(
        storage: &Storage,
        config: &AppConfig,
        audio_outputs: Vec<String>,
    ) -> Result<Self> {
        let queue = storage.load_queue()?;
        let queue_index = queue.current_index;
        let now_playing = queue_index.and_then(|index| queue.tracks.get(index).cloned());
        Ok(Self {
            screen: Screen::Home,
            selected: 0,
            search_query: String::new(),
            search_results: Vec::new(),
            wave_tracks: Vec::new(),
            wave_loading: false,
            library: storage.library_tracks()?,
            playlists: storage.list_playlists()?,
            queue: queue.tracks,
            queue_index,
            now_playing,
            player: PlayerState {
                volume_percent: config.volume_percent,
                shuffle: queue.shuffle,
                repeat: queue.repeat,
                ..PlayerState::default()
            },
            credentials: CredentialState::default(),
            soundcloud_enabled: config.soundcloud_enabled,
            yandex_enabled: config.yandex_enabled,
            modal: (!config.onboarding_completed).then(|| {
                Modal::Onboarding(Box::new(OnboardingState::with_audio_outputs(
                    config.audio_output.clone(),
                    audio_outputs,
                )))
            }),
            should_quit: false,
            dirty: true,
            queue_dirty: false,
            config_dirty: false,
            status_message: "Готово".to_string(),
            onboarding_result: None,
            effects: Vec::new(),
        })
    }

    pub fn handle(&mut self, action: Action) {
        match action {
            Action::Quit => self.should_quit = true,
            Action::Navigate(screen) => {
                self.screen = screen;
                self.selected = 0;
                if screen == Screen::Wave && self.wave_tracks.is_empty() && !self.wave_loading {
                    self.wave_loading = true;
                    self.status_message = "Собираем Мою волну".to_string();
                    self.effects.push(AppEffect::GenerateWave);
                }
            }
            Action::SelectPrevious => {
                self.selected = self.selected.saturating_sub(1);
            }
            Action::SelectNext => {
                self.selected = self
                    .selected
                    .saturating_add(1)
                    .min(self.item_count().saturating_sub(1));
            }
            Action::Activate => self.activate_selected(),
            Action::TogglePause => {
                if self.now_playing.is_some() {
                    let (status, effect) = match self.player.status {
                        PlaybackStatus::Playing | PlaybackStatus::Buffering => {
                            (PlaybackStatus::Paused, AppEffect::Pause)
                        }
                        _ => (PlaybackStatus::Playing, AppEffect::Resume),
                    };
                    self.player.status = status;
                    self.effects.push(effect);
                }
            }
            Action::NextTrack => self.next_track(),
            Action::PreviousTrack => self.previous_track(),
            Action::ChangeVolume(delta) => {
                self.player.volume_percent =
                    (i16::from(self.player.volume_percent) + i16::from(delta)).clamp(0, 100) as u8;
                self.effects
                    .push(AppEffect::SetVolume(self.player.volume_percent));
                self.config_dirty = true;
            }
            Action::Seek(delta) => {
                self.player.position_ms = (self.player.position_ms as i64 + delta)
                    .clamp(0, self.player.duration_ms as i64)
                    as u64;
                if self.now_playing.is_some() {
                    self.effects.push(AppEffect::Seek(self.player.position_ms));
                }
            }
            Action::ToggleShuffle => {
                self.player.shuffle = !self.player.shuffle;
                self.queue_dirty = true;
            }
            Action::CycleRepeat => {
                self.player.repeat = match self.player.repeat {
                    RepeatMode::Off => RepeatMode::All,
                    RepeatMode::All => RepeatMode::One,
                    RepeatMode::One => RepeatMode::Off,
                };
                self.queue_dirty = true;
            }
            Action::StartSearch => {
                self.screen = Screen::Search;
                self.selected = 0;
            }
            Action::SearchInput(value) => {
                self.search_query.push(value);
                self.schedule_search(false);
            }
            Action::SearchBackspace => {
                self.search_query.pop();
                self.schedule_search(false);
            }
            Action::SubmitSearch => {
                self.status_message = if self.search_query.trim().is_empty() {
                    "Введите запрос".to_string()
                } else {
                    let query = self.search_query.trim().to_string();
                    self.effects.push(AppEffect::Search {
                        query: query.clone(),
                        immediate: true,
                    });
                    format!("Ищем: {query}")
                };
            }
            Action::SearchFinished {
                query,
                tracks,
                failures,
            } => self.finish_search(query, tracks, failures),
            Action::WaveFinished { tracks, failures } => {
                self.wave_tracks = tracks;
                self.wave_loading = false;
                self.selected = 0;
                self.status_message = if self.wave_tracks.is_empty() && !failures.is_empty() {
                    failures.join("; ")
                } else if self.wave_tracks.is_empty() {
                    "Волна пока пустая, послушай или лайкни несколько треков".to_string()
                } else if failures.is_empty() {
                    format!("В волне {} треков", self.wave_tracks.len())
                } else {
                    format!(
                        "В волне {} треков, часть сервисов прилегла: {}",
                        self.wave_tracks.len(),
                        failures.join(", ")
                    )
                };
            }
            Action::AudioProgress {
                position_ms,
                buffered_ms,
            } => {
                self.player.position_ms = if self.player.duration_ms == 0 {
                    position_ms
                } else {
                    position_ms.min(self.player.duration_ms)
                };
                self.player.buffered_ms = buffered_ms;
            }
            Action::Audio(event) => self.handle_audio_event(event),
            Action::PlaybackFailed(error) => {
                self.player.status = PlaybackStatus::Stopped;
                self.status_message = format!("Не удалось включить трек: {error}");
            }
            Action::OpenHelp => self.modal = Some(Modal::Help),
            Action::OpenCommandPalette => self.modal = Some(Modal::CommandPalette),
            Action::OpenPlaylistImport => {
                self.modal = Some(Modal::PlaylistImport(Box::default()));
                self.status_message = "Вставь ссылку на плейлист SoundCloud или Yandex".to_string();
            }
            Action::CloseModal => self.close_modal(),
            Action::ModalSubmit => self.submit_modal(),
            Action::ModalPrevious => self.move_modal_selection(false),
            Action::ModalNext => self.move_modal_selection(true),
            Action::ModalToggle => self.toggle_modal(),
            Action::ModalInput(value) => self.input_modal(value),
            Action::ModalBackspace => self.backspace_modal(),
            Action::CredentialSaved { kind, result } => self.finish_credential_save(kind, result),
            Action::PlaylistImported(result) => self.finish_playlist_import(result),
            Action::SoundCloudChecked(access) => {
                self.update_onboarding(|state| state.soundcloud_checked(access));
            }
            Action::ZapretPlanned(result) => {
                self.update_onboarding(|state| {
                    state.zapret_planned(result.map(|plan| *plan));
                    OnboardingCommand::None
                });
            }
            Action::ZapretApplied(result) => {
                let status = result.as_ref().ok().map(|result| {
                    if let Some(backup) = &result.backup_path {
                        format!(
                            "Домены добавлены, backup: {}. Перезапусти Zapret вручную",
                            backup.display()
                        )
                    } else {
                        "Домены добавлены. Перезапусти Zapret вручную".to_string()
                    }
                });
                self.update_onboarding(|state| state.zapret_applied(result.map(|_| ())));
                if let Some(status) = status {
                    self.status_message = status;
                }
            }
            Action::AudioOutputChanged(result) => {
                self.status_message = match result {
                    Ok(name) => format!("Аудиовыход готов: {name}"),
                    Err(error) => format!("Аудиовыход не переключился: {error}"),
                };
            }
            Action::Tick => {}
            Action::Resize => {}
        }
        self.dirty = true;
    }

    pub fn queue_snapshot(&self) -> QueueSnapshot {
        QueueSnapshot {
            tracks: self.queue.clone(),
            current_index: self.queue_index,
            shuffle: self.player.shuffle,
            repeat: self.player.repeat,
        }
    }

    pub fn take_effects(&mut self) -> Vec<AppEffect> {
        std::mem::take(&mut self.effects)
    }

    pub fn onboarding_open(&self) -> bool {
        matches!(self.modal, Some(Modal::Onboarding(_)))
    }

    pub fn take_onboarding_result(&mut self) -> Option<OnboardingResult> {
        self.onboarding_result.take()
    }

    pub fn set_credentials(&mut self, credentials: CredentialState) {
        self.credentials = credentials;
        self.dirty = true;
    }

    pub fn selected_tracks(&self) -> &[TrackRef] {
        match self.screen {
            Screen::Search => &self.search_results,
            Screen::Wave => &self.wave_tracks,
            Screen::Library | Screen::Home => &self.library,
            Screen::Queue => &self.queue,
            _ => &[],
        }
    }

    fn item_count(&self) -> usize {
        match self.screen {
            Screen::Playlists => self.playlists.len(),
            Screen::Settings => CredentialKind::ALL.len(),
            _ => self.selected_tracks().len(),
        }
    }

    fn schedule_search(&mut self, immediate: bool) {
        let query = self.search_query.trim().to_string();
        if query.is_empty() || !immediate {
            self.search_results.clear();
            self.selected = 0;
        }
        if query.is_empty() {
            self.status_message = "Введите запрос".to_string();
        }
        self.effects.push(AppEffect::Search { query, immediate });
    }

    fn finish_search(&mut self, query: String, tracks: Vec<TrackRef>, failures: Vec<String>) {
        if self.search_query.trim() != query {
            return;
        }
        self.search_results = tracks;
        self.selected = 0;
        self.status_message = if query.is_empty() {
            "Введите запрос".to_string()
        } else if self.search_results.is_empty() && !failures.is_empty() {
            failures.join("; ")
        } else if self.search_results.is_empty() {
            "Ничего не найдено".to_string()
        } else if failures.is_empty() {
            format!("Найдено: {}", self.search_results.len())
        } else {
            format!(
                "Найдено: {}, часть сервисов прилегла: {}",
                self.search_results.len(),
                failures.join(", ")
            )
        };
    }

    fn handle_audio_event(&mut self, event: crate::audio::AudioEvent) {
        match event {
            crate::audio::AudioEvent::Buffering => {
                self.player.status = PlaybackStatus::Buffering;
                self.status_message = "Буферизация".to_string();
            }
            crate::audio::AudioEvent::Playing => {
                self.player.status = PlaybackStatus::Playing;
                self.status_message = "Воспроизведение".to_string();
            }
            crate::audio::AudioEvent::Paused => self.player.status = PlaybackStatus::Paused,
            crate::audio::AudioEvent::Stopped => self.player.status = PlaybackStatus::Stopped,
            crate::audio::AudioEvent::Ended => self.next_track(),
            crate::audio::AudioEvent::Failed(error)
            | crate::audio::AudioEvent::OutputFailed(error) => {
                self.player.status = PlaybackStatus::Stopped;
                self.status_message = format!("Аудио сломалось: {error}");
            }
        }
    }

    fn activate_selected(&mut self) {
        if self.screen == Screen::Settings {
            if let Some(kind) = CredentialKind::ALL.get(self.selected).copied() {
                self.modal = Some(Modal::Credential(Box::new(CredentialEditor::new(kind))));
                self.status_message = format!("Введи новый {}", kind.label());
            }
            return;
        }
        let Some(track) = self.selected_tracks().get(self.selected).cloned() else {
            return;
        };
        if let Some(index) = self
            .queue
            .iter()
            .position(|current| current.provider_key() == track.provider_key())
        {
            self.queue_index = Some(index);
        } else {
            self.queue.push(track.clone());
            self.queue_index = Some(self.queue.len() - 1);
        }
        self.now_playing = Some(track.clone());
        self.player.position_ms = 0;
        self.player.buffered_ms = 0;
        self.player.duration_ms = track.duration_ms.unwrap_or_default();
        self.player.status = PlaybackStatus::Buffering;
        self.queue_dirty = true;
        self.effects.push(AppEffect::Play(Box::new(track)));
    }

    fn next_track(&mut self) {
        if self.queue.is_empty() {
            return;
        }
        // repeat one увидел очередь и сказал НЕТ БРАТАН Я ТУТ ГЛАВНЫЙ )))))
        let current = self.queue_index.unwrap_or(0);
        let next = if self.player.repeat == RepeatMode::One {
            current
        } else if current + 1 < self.queue.len() {
            current + 1
        } else if self.player.repeat == RepeatMode::All {
            0
        } else {
            self.player.status = PlaybackStatus::Stopped;
            self.effects.push(AppEffect::Stop);
            return;
        };
        self.set_queue_track(next);
    }

    fn previous_track(&mut self) {
        if self.queue.is_empty() {
            return;
        }
        // пять секунд это ещё не прошлый трек, это палец случайно решил пожить 🫩
        if self.player.position_ms > 5_000 {
            self.player.position_ms = 0;
            self.effects.push(AppEffect::Seek(0));
            return;
        }
        let current = self.queue_index.unwrap_or(0);
        let previous = if current > 0 {
            current - 1
        } else if self.player.repeat == RepeatMode::All {
            self.queue.len() - 1
        } else {
            0
        };
        self.set_queue_track(previous);
    }

    fn set_queue_track(&mut self, index: usize) {
        self.queue_index = Some(index);
        self.now_playing = self.queue.get(index).cloned();
        self.player.position_ms = 0;
        self.player.buffered_ms = 0;
        self.player.duration_ms = self
            .now_playing
            .as_ref()
            .and_then(|track| track.duration_ms)
            .unwrap_or_default();
        self.player.status = PlaybackStatus::Buffering;
        self.queue_dirty = true;
        if let Some(track) = self.now_playing.clone() {
            self.effects.push(AppEffect::Play(Box::new(track)));
        }
    }

    fn update_onboarding(
        &mut self,
        update: impl FnOnce(&mut OnboardingState) -> OnboardingCommand,
    ) {
        let command = match self.modal.as_mut() {
            Some(Modal::Onboarding(state)) => update(state),
            _ => return,
        };
        self.handle_onboarding_command(command);
    }

    fn submit_modal(&mut self) {
        match self.modal.as_mut() {
            Some(Modal::Onboarding(_)) => self.update_onboarding(OnboardingState::confirm),
            Some(Modal::Credential(editor)) => {
                let value = editor.value.trim().to_string();
                if value.is_empty() {
                    self.status_message =
                        "Пустой ключ сохранять не будем, цирк уже занят".to_string();
                    return;
                }
                editor.saving = true;
                self.status_message = format!("Сохраняем {}", editor.kind.label());
                self.effects.push(AppEffect::SaveCredential {
                    kind: editor.kind,
                    value,
                });
            }
            Some(Modal::PlaylistImport(editor)) => {
                let url = editor.url.trim().to_string();
                if url.is_empty() {
                    self.status_message =
                        "Ссылка пустая, импортировать телепатию пока нельзя".to_string();
                    return;
                }
                editor.loading = true;
                self.status_message = "Импортируем плейлист".to_string();
                self.effects.push(AppEffect::ImportPlaylist(url));
            }
            _ => self.close_modal(),
        }
    }

    fn move_modal_selection(&mut self, next: bool) {
        self.update_onboarding(|state| {
            if next {
                state.select_next();
            } else {
                state.select_previous();
            }
            OnboardingCommand::None
        });
    }

    fn toggle_modal(&mut self) {
        self.update_onboarding(|state| {
            state.toggle();
            OnboardingCommand::None
        });
    }

    fn input_modal(&mut self, value: char) {
        match self.modal.as_mut() {
            Some(Modal::Onboarding(state)) => state.input(value),
            Some(Modal::Credential(editor)) => editor.input(value),
            Some(Modal::PlaylistImport(editor)) => editor.input(value),
            _ => {}
        }
    }

    fn backspace_modal(&mut self) {
        match self.modal.as_mut() {
            Some(Modal::Onboarding(state)) => state.backspace(),
            Some(Modal::Credential(editor)) => editor.backspace(),
            Some(Modal::PlaylistImport(editor)) => editor.backspace(),
            _ => {}
        }
    }

    fn finish_credential_save(&mut self, kind: CredentialKind, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.credentials.set_configured(kind, true);
                match kind {
                    CredentialKind::SoundCloudClientId => self.soundcloud_enabled = true,
                    CredentialKind::YandexToken => self.yandex_enabled = true,
                }
                self.config_dirty = true;
                self.modal = None;
                self.status_message = format!("{} сохранён, сервис уже перезапущен", kind.label());
            }
            Err(error) => {
                if let Some(Modal::Credential(editor)) = self.modal.as_mut() {
                    editor.saving = false;
                }
                self.status_message = format!("Не удалось сохранить {}: {error}", kind.label());
            }
        }
    }

    fn finish_playlist_import(&mut self, result: Result<Playlist, String>) {
        match result {
            Ok(playlist) => {
                let title = playlist.title.clone();
                let count = playlist.tracks.len();
                if let Some(index) = self
                    .playlists
                    .iter()
                    .position(|current| current.id == playlist.id)
                {
                    self.playlists[index] = playlist;
                } else {
                    self.playlists.insert(0, playlist);
                }
                self.screen = Screen::Playlists;
                self.selected = 0;
                self.modal = None;
                self.status_message = format!("Импортирован «{title}»: {count} треков");
            }
            Err(error) => {
                if let Some(Modal::PlaylistImport(editor)) = self.modal.as_mut() {
                    editor.loading = false;
                }
                self.status_message = format!("Импорт не удался: {error}");
            }
        }
    }

    fn handle_onboarding_command(&mut self, command: OnboardingCommand) {
        match command {
            OnboardingCommand::None => {}
            OnboardingCommand::ProbeSoundCloud => {
                self.status_message = "Проверяем доступ к SoundCloud".to_string();
                let output = match &self.modal {
                    Some(Modal::Onboarding(state)) => state.audio_output.clone(),
                    _ => None,
                };
                self.effects.push(AppEffect::SelectAudioOutput {
                    output,
                    volume_percent: self.player.volume_percent,
                });
                self.effects.push(AppEffect::ProbeSoundCloud);
            }
            OnboardingCommand::StartAccountLogin => {
                self.status_message =
                    "Вход в аккаунт продолжится после подключения серверного API".to_string();
            }
            OnboardingCommand::PlanZapret(path) => {
                self.status_message = "Проверяем установку Zapret".to_string();
                self.effects.push(AppEffect::PlanZapret(path));
            }
            OnboardingCommand::ApplyZapret(plan) => {
                self.status_message = "Добавляем домены в пользовательский список".to_string();
                self.effects.push(AppEffect::ApplyZapret(plan));
            }
            OnboardingCommand::Finish => {
                if let Some(Modal::Onboarding(state)) = &self.modal {
                    self.onboarding_result = Some(state.result());
                }
                self.modal = None;
                self.config_dirty = true;
            }
        }
    }

    fn close_modal(&mut self) {
        if let Some(Modal::Onboarding(state)) = &self.modal {
            let mut result = state.result();
            if state.step == crate::onboarding::OnboardingStep::Welcome {
                result.account_mode = AccountMode::Guest;
            }
            self.onboarding_result = Some(result);
            self.config_dirty = true;
        }
        self.modal = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::AppConfig,
        effect::AppEffect,
        model::{PlaybackCapability, ProviderKind},
    };
    use url::Url;

    #[test]
    fn navigation_wrap_is_clamped_to_empty_content() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path().join("db.sqlite3"));
        storage.initialize().unwrap();
        let mut app = App::load(&storage, &AppConfig::default()).unwrap();
        app.handle(Action::SelectNext);
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn activating_track_emits_play_effect() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path().join("db.sqlite3"));
        storage.initialize().unwrap();
        let mut app = App::load(&storage, &AppConfig::default()).unwrap();
        let track = test_track();
        app.library.push(track.clone());
        app.screen = Screen::Library;
        app.handle(Action::Activate);
        assert_eq!(app.take_effects(), vec![AppEffect::Play(Box::new(track))]);
    }

    #[test]
    fn search_submission_emits_trimmed_query() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path().join("db.sqlite3"));
        storage.initialize().unwrap();
        let mut app = App::load(&storage, &AppConfig::default()).unwrap();
        app.search_query = "  winter mix  ".to_string();
        app.handle(Action::SubmitSearch);
        assert_eq!(
            app.take_effects(),
            vec![AppEffect::Search {
                query: "winter mix".to_string(),
                immediate: true
            }]
        );
    }

    #[test]
    fn settings_editor_emits_secret_save_without_putting_value_in_state() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path().join("db.sqlite3"));
        storage.initialize().unwrap();
        let mut app = App::load(
            &storage,
            &AppConfig {
                onboarding_completed: true,
                ..AppConfig::default()
            },
        )
        .unwrap();
        app.handle(Action::Navigate(Screen::Settings));
        app.handle(Action::Activate);
        for value in "client-id".chars() {
            app.handle(Action::ModalInput(value));
        }
        app.handle(Action::ModalSubmit);

        assert_eq!(
            app.take_effects(),
            vec![AppEffect::SaveCredential {
                kind: CredentialKind::SoundCloudClientId,
                value: "client-id".to_string(),
            }]
        );
    }

    #[test]
    fn imported_playlist_opens_the_real_playlist_screen() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path().join("db.sqlite3"));
        storage.initialize().unwrap();
        let mut app = App::load(
            &storage,
            &AppConfig {
                onboarding_completed: true,
                ..AppConfig::default()
            },
        )
        .unwrap();
        let mut playlist = Playlist::new("Микс", 1);
        playlist.push_unique(test_track());

        app.handle(Action::PlaylistImported(Ok(playlist)));

        assert_eq!(app.screen, Screen::Playlists);
        assert_eq!(app.playlists.len(), 1);
        assert!(app.status_message.contains("1 треков"));
    }

    #[test]
    fn first_run_checks_soundcloud_and_skips_zapret_when_it_answers() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path().join("db.sqlite3"));
        storage.initialize().unwrap();
        let mut app = App::load(&storage, &AppConfig::default()).unwrap();

        app.handle(Action::ModalSubmit);
        app.handle(Action::ModalNext);
        app.handle(Action::ModalSubmit);
        app.handle(Action::ModalNext);
        app.handle(Action::ModalNext);
        app.handle(Action::ModalSubmit);
        app.handle(Action::ModalSubmit);

        assert_eq!(
            app.take_effects(),
            vec![
                AppEffect::SelectAudioOutput {
                    output: None,
                    volume_percent: 75,
                },
                AppEffect::ProbeSoundCloud,
            ]
        );
        app.handle(Action::SoundCloudChecked(
            crate::onboarding::SoundCloudAccess::Reachable { status: 401 },
        ));
        assert!(!app.onboarding_open());
        let result = app.take_onboarding_result().unwrap();
        assert_eq!(result.account_mode, AccountMode::Guest);
        assert!(result.soundcloud_enabled);
    }

    #[test]
    fn failed_soundcloud_check_keeps_wizard_open_for_zapret_choice() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path().join("db.sqlite3"));
        storage.initialize().unwrap();
        let mut app = App::load(&storage, &AppConfig::default()).unwrap();
        if let Some(Modal::Onboarding(state)) = app.modal.as_mut() {
            state.step = crate::onboarding::OnboardingStep::CheckingSoundCloud;
        }

        app.handle(Action::SoundCloudChecked(
            crate::onboarding::SoundCloudAccess::Unreachable {
                reason: "соединение закрыто".to_string(),
            },
        ));

        assert!(matches!(
            app.modal,
            Some(Modal::Onboarding(ref state))
                if state.step == crate::onboarding::OnboardingStep::ZapretChoice
        ));
    }

    #[test]
    fn stale_search_result_goes_away_without_a_speech() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path().join("db.sqlite3"));
        storage.initialize().unwrap();
        let mut app = App::load(&storage, &AppConfig::default()).unwrap();
        app.search_query = "новый запрос".to_string();
        app.handle(Action::SearchFinished {
            query: "старый запрос".to_string(),
            tracks: vec![test_track()],
            failures: Vec::new(),
        });
        assert!(app.search_results.is_empty());
    }

    #[test]
    fn ended_audio_moves_queue_forward() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path().join("db.sqlite3"));
        storage.initialize().unwrap();
        let mut app = App::load(&storage, &AppConfig::default()).unwrap();
        app.queue = vec![
            test_track(),
            TrackRef {
                id: "43".to_string(),
                ..test_track()
            },
        ];
        app.queue_index = Some(0);
        app.now_playing = app.queue.first().cloned();
        app.handle(Action::Audio(crate::audio::AudioEvent::Ended));
        assert_eq!(app.queue_index, Some(1));
        assert!(matches!(
            app.take_effects().as_slice(),
            [AppEffect::Play(_)]
        ));
    }

    #[test]
    fn opening_wave_starts_local_generation_once() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path().join("db.sqlite3"));
        storage.initialize().unwrap();
        let mut app = App::load(&storage, &AppConfig::default()).unwrap();
        app.handle(Action::Navigate(Screen::Wave));
        app.handle(Action::Navigate(Screen::Wave));
        assert!(app.wave_loading);
        assert_eq!(app.take_effects(), vec![AppEffect::GenerateWave]);
    }

    #[test]
    fn finished_wave_gets_its_own_list_instead_of_library_costume() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path().join("db.sqlite3"));
        storage.initialize().unwrap();
        let mut app = App::load(&storage, &AppConfig::default()).unwrap();
        app.handle(Action::Navigate(Screen::Wave));
        app.handle(Action::WaveFinished {
            tracks: vec![test_track()],
            failures: Vec::new(),
        });
        assert_eq!(app.selected_tracks(), [test_track()]);
        assert!(!app.wave_loading);
    }

    fn test_track() -> TrackRef {
        TrackRef {
            provider: ProviderKind::SoundCloud,
            id: "42".to_string(),
            title: "Трек".to_string(),
            artists: vec!["Автор".to_string()],
            duration_ms: Some(60_000),
            artwork_url: None,
            web_url: Url::parse("https://soundcloud.com/test/track").unwrap(),
            capability: PlaybackCapability::Full,
            genres: Vec::new(),
            explicit: false,
        }
    }
}
