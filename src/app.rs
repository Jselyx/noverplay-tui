use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;

use crate::{
    account::{
        error::AccountApiError,
        models::{AccountAction, AccountSession, BootstrapUpdate, CaptchaChallenge},
        state::{AccountDialog, AccountDialogStage, AccountState},
    },
    action::Action,
    command_palette::{CommandPalette, PaletteCommand},
    config::{AppConfig, HotkeyBindings},
    credentials::{CredentialEditor, CredentialKind, CredentialState},
    effect::AppEffect,
    hotkeys::{HotkeyAction, HotkeyEditor},
    importer::PlaylistImportEditor,
    model::{Playlist, RepeatMode, SearchProvider, TrackRef},
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
    Account(Box<AccountDialog>),
    Help,
    Keybindings,
    CommandPalette(Box<CommandPalette>),
    Hotkey(Box<HotkeyEditor>),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum BootstrapState {
    #[default]
    Unknown,
    Refreshing,
    Ready {
        refresh_at: Option<String>,
    },
    Failed(String),
}

#[derive(Debug)]
pub struct App {
    pub screen: Screen,
    pub selected: usize,
    pub search_query: String,
    pub search_input_focused: bool,
    pub search_provider: SearchProvider,
    pub search_results: Vec<TrackRef>,
    pub home_tracks: Vec<TrackRef>,
    pub wave_tracks: Vec<TrackRef>,
    pub wave_loading: bool,
    pub library: Vec<TrackRef>,
    pub playlists: Vec<Playlist>,
    pub active_playlist: Option<uuid::Uuid>,
    pub queue: Vec<TrackRef>,
    pub queue_index: Option<usize>,
    pub now_playing: Option<TrackRef>,
    pub player: PlayerState,
    pub credentials: CredentialState,
    pub account: AccountState,
    pub bootstrap: BootstrapState,
    pub soundcloud_refresh_at_ms: Option<i64>,
    pub soundcloud_enabled: bool,
    pub yandex_enabled: bool,
    pub deezer_enabled: bool,
    pub global_hotkeys_enabled: bool,
    pub hotkeys: HotkeyBindings,
    pub keybindings_notice_seen: bool,
    pub modal: Option<Modal>,
    pub should_quit: bool,
    pub dirty: bool,
    pub queue_dirty: bool,
    pub config_dirty: bool,
    pub status_message: String,
    onboarding_result: Option<OnboardingResult>,
    onboarding_resume: Option<Box<OnboardingState>>,
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
            search_input_focused: true,
            search_provider: SearchProvider::All,
            search_results: Vec::new(),
            home_tracks: storage
                .recent_history(24)?
                .into_iter()
                .map(|entry| entry.track)
                .collect(),
            wave_tracks: Vec::new(),
            wave_loading: false,
            library: storage.library_tracks()?,
            playlists: storage.list_playlists()?,
            active_playlist: None,
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
            account: AccountState::Guest,
            bootstrap: config
                .soundcloud_client_id_refresh_at_ms
                .filter(|deadline| *deadline > now_ms())
                .map(|_| BootstrapState::Ready { refresh_at: None })
                .unwrap_or_default(),
            soundcloud_refresh_at_ms: config.soundcloud_client_id_refresh_at_ms,
            soundcloud_enabled: config.soundcloud_enabled,
            yandex_enabled: config.yandex_enabled,
            deezer_enabled: config.deezer_enabled,
            global_hotkeys_enabled: config.global_hotkeys_enabled,
            hotkeys: config.hotkeys.clone(),
            keybindings_notice_seen: config.keybindings_notice_seen,
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
            onboarding_resume: None,
            effects: Vec::new(),
        })
    }

    pub fn handle(&mut self, action: Action) {
        match action {
            Action::Quit => self.should_quit = true,
            Action::Navigate(screen) => {
                self.screen = screen;
                if screen != Screen::Playlists {
                    self.active_playlist = None;
                }
                self.selected = 0;
                if screen == Screen::Wave && self.wave_tracks.is_empty() && !self.wave_loading {
                    self.wave_loading = true;
                    self.status_message = "Собираем Мою волну".to_string();
                    self.effects.push(AppEffect::GenerateWave);
                }
            }
            Action::Back => {
                if self.screen == Screen::Playlists && self.active_playlist.take().is_some() {
                    self.selected = 0;
                    self.status_message = "Список плейлистов".to_string();
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
            Action::ToggleLike => self.toggle_like(),
            Action::StartSearch => {
                self.screen = Screen::Search;
                self.search_input_focused = true;
                self.selected = 0;
            }
            Action::ToggleSearchFocus => {
                self.search_input_focused = !self.search_input_focused;
                self.status_message = if self.search_input_focused {
                    "Ввод запроса · Ctrl+J к результатам".to_string()
                } else {
                    "Навигация по результатам · / вернуться к вводу".to_string()
                };
            }
            Action::FocusSearchInput => self.search_input_focused = true,
            Action::CycleSearchProvider => {
                self.search_provider = self.search_provider.next();
                self.schedule_search(false);
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
                        provider: self.search_provider,
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
            Action::ControlSearchFinished { .. } => {}
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
            Action::ControlWaveFinished { .. } => {}
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
            Action::OpenKeybindings => self.modal = Some(Modal::Keybindings),
            Action::OpenCommandPalette => self.modal = Some(Modal::CommandPalette(Box::default())),
            Action::OpenPlaylistImport => {
                self.modal = Some(Modal::PlaylistImport(Box::default()));
                self.status_message =
                    "Вставь ссылку на плейлист SoundCloud, Yandex или Deezer".to_string();
            }
            Action::OpenAccount(action) => {
                self.modal = Some(Modal::Account(Box::new(AccountDialog::new(action))));
                self.status_message = "Введи логин и пароль Noverplay".to_string();
            }
            Action::ToggleAccountMode => self.toggle_account_mode(),
            Action::AccountLogout => self.logout_account(),
            Action::MouseClick {
                column,
                row,
                terminal_width,
                terminal_height,
            } => self.click_modal(column, row, terminal_width, terminal_height),
            Action::CloseModal => self.close_modal(),
            Action::ModalSubmit => self.submit_modal(),
            Action::ModalPrevious => self.move_modal_selection(false),
            Action::ModalNext => self.move_modal_selection(true),
            Action::ModalToggle => self.toggle_modal(),
            Action::ModalInput(value) => self.input_modal(value),
            Action::ModalBackspace => self.backspace_modal(),
            Action::CredentialSaved { kind, result } => self.finish_credential_save(kind, result),
            Action::PlaylistImported(result) => self.finish_playlist_import(result),
            Action::LikeSaved {
                track,
                liked,
                result,
            } => self.finish_like(*track, liked, result),
            Action::AccountCaptchaLoaded { action, result } => {
                self.finish_account_captcha(action, result)
            }
            Action::AccountAuthenticated(result) => self.finish_authentication(result),
            Action::AccountRestored(result) => self.finish_account_restore(result),
            Action::BootstrapFinished(result) => self.finish_bootstrap(result),
            Action::AccountLoggedOut {
                result,
                soundcloud_configured,
            } => self.finish_logout(result, soundcloud_configured),
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

    pub fn control_search(&mut self, query: String, provider: SearchProvider) {
        self.effects
            .push(AppEffect::ControlSearch { query, provider });
    }

    pub fn control_wave(&mut self) {
        self.effects.push(AppEffect::ControlWave);
    }

    pub fn control_play(&mut self, track: TrackRef) {
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

    pub fn control_play_wave(&mut self, tracks: Vec<TrackRef>) -> Option<(TrackRef, usize)> {
        let first = tracks.first()?.clone();
        let count = tracks.len();
        self.wave_tracks = tracks.clone();
        self.queue = tracks;
        self.queue_index = Some(0);
        self.now_playing = Some(first.clone());
        self.player.position_ms = 0;
        self.player.buffered_ms = 0;
        self.player.duration_ms = first.duration_ms.unwrap_or_default();
        self.player.status = PlaybackStatus::Buffering;
        self.queue_dirty = true;
        self.effects.push(AppEffect::Play(Box::new(first.clone())));
        Some((first, count))
    }

    pub fn control_queue_add(&mut self, track: TrackRef) -> usize {
        self.queue.push(track);
        self.queue_dirty = true;
        self.queue.len()
    }

    pub fn control_queue_remove(&mut self, position: usize) -> Result<TrackRef, String> {
        let index = position
            .checked_sub(1)
            .filter(|index| *index < self.queue.len())
            .ok_or_else(|| format!("в очереди нет позиции {position}"))?;
        let removed = self.queue.remove(index);
        self.queue_index = match self.queue_index {
            Some(current) if current == index => {
                self.now_playing = None;
                self.player.status = PlaybackStatus::Stopped;
                self.effects.push(AppEffect::Stop);
                None
            }
            Some(current) if current > index => Some(current - 1),
            Some(current) if current < self.queue.len() => Some(current),
            _ => None,
        };
        self.queue_dirty = true;
        Ok(removed)
    }

    pub fn control_queue_clear(&mut self) {
        self.queue.clear();
        self.queue_index = None;
        self.now_playing = None;
        self.player.status = PlaybackStatus::Stopped;
        self.queue_dirty = true;
        self.effects.push(AppEffect::Stop);
    }

    pub fn control_pause(&mut self) -> Result<(), String> {
        if self.now_playing.is_none() {
            return Err("сейчас ничего не играет".to_string());
        }
        self.player.status = PlaybackStatus::Paused;
        self.effects.push(AppEffect::Pause);
        Ok(())
    }

    pub fn control_resume(&mut self) -> Result<(), String> {
        let Some(track) = self.now_playing.clone() else {
            return Err("нечего продолжать".to_string());
        };
        if self.player.status == PlaybackStatus::Stopped {
            self.player.position_ms = 0;
            self.player.buffered_ms = 0;
            self.player.status = PlaybackStatus::Buffering;
            self.effects.push(AppEffect::Play(Box::new(track)));
        } else {
            self.player.status = PlaybackStatus::Playing;
            self.effects.push(AppEffect::Resume);
        }
        Ok(())
    }

    pub fn control_stop(&mut self) {
        self.player.status = PlaybackStatus::Stopped;
        self.effects.push(AppEffect::Stop);
    }

    pub fn control_toggle(&mut self) -> Result<(), String> {
        if matches!(
            self.player.status,
            PlaybackStatus::Playing | PlaybackStatus::Buffering
        ) {
            self.control_pause()
        } else {
            self.control_resume()
        }
    }

    pub fn control_next(&mut self) {
        self.next_track();
    }

    pub fn control_previous(&mut self) {
        self.previous_track();
    }

    pub fn take_effects(&mut self) -> Vec<AppEffect> {
        std::mem::take(&mut self.effects)
    }

    pub fn onboarding_open(&self) -> bool {
        matches!(self.modal, Some(Modal::Onboarding(_))) || self.onboarding_resume.is_some()
    }

    pub fn text_modal_open(&self) -> bool {
        matches!(
            self.modal,
            Some(
                Modal::Credential(_)
                    | Modal::PlaylistImport(_)
                    | Modal::Account(_)
                    | Modal::Hotkey(_)
            )
        )
    }

    pub fn restore_account(&mut self) {
        self.account = AccountState::Restoring;
        self.effects.push(AppEffect::RestoreAccount);
        self.status_message = "Проверяем сессию Noverplay".to_string();
        self.dirty = true;
    }

    pub fn take_onboarding_result(&mut self) -> Option<OnboardingResult> {
        self.onboarding_result.take()
    }

    pub fn show_keybindings_notice_if_needed(&mut self) {
        if !self.keybindings_notice_seen && self.modal.is_none() {
            self.modal = Some(Modal::Keybindings);
            self.status_message = "Кейбинды всегда можно открыть через Ctrl+9".to_string();
            self.dirty = true;
        }
    }

    pub fn set_credentials(&mut self, credentials: CredentialState) {
        self.credentials = credentials;
        self.dirty = true;
    }

    pub fn status_snapshot(&self) -> crate::control::StatusSnapshot {
        crate::control::StatusSnapshot {
            playback: format!("{:?}", self.player.status).to_ascii_lowercase(),
            track: self.now_playing.clone(),
            position_ms: self.player.position_ms,
            duration_ms: self.player.duration_ms,
            volume_percent: self.player.volume_percent,
            queue_index: self.queue_index,
            queue_length: self.queue.len(),
        }
    }

    pub fn selected_tracks(&self) -> &[TrackRef] {
        match self.screen {
            Screen::Search => &self.search_results,
            Screen::Wave => &self.wave_tracks,
            Screen::Library => &self.library,
            Screen::Home if !self.home_tracks.is_empty() => &self.home_tracks,
            Screen::Home => &self.library,
            Screen::Queue => &self.queue,
            Screen::Playlists => self
                .active_playlist
                .and_then(|id| self.playlists.iter().find(|playlist| playlist.id == id))
                .map(|playlist| playlist.tracks.as_slice())
                .unwrap_or(&[]),
            _ => &[],
        }
    }

    fn item_count(&self) -> usize {
        match self.screen {
            Screen::Playlists => self
                .active_playlist
                .and_then(|id| self.playlists.iter().find(|playlist| playlist.id == id))
                .map(|playlist| playlist.tracks.len())
                .unwrap_or(self.playlists.len()),
            Screen::Settings => CredentialKind::ALL.len() + 1 + HotkeyAction::ALL.len(),
            Screen::Profile => 1,
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
        self.effects.push(AppEffect::Search {
            query,
            provider: self.search_provider,
            immediate,
        });
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
        if self.screen == Screen::Playlists && self.active_playlist.is_none() {
            if let Some(playlist) = self.playlists.get(self.selected) {
                self.active_playlist = Some(playlist.id);
                self.selected = 0;
                self.status_message = format!(
                    "Плейлист «{}»: {} треков, Esc назад",
                    playlist.title,
                    playlist.tracks.len()
                );
            }
            return;
        }
        if self.screen == Screen::Profile {
            match &self.account {
                AccountState::Guest => {
                    self.modal = Some(Modal::Account(Box::new(AccountDialog::new(
                        AccountAction::Login,
                    ))));
                    self.status_message = "Введи логин и пароль Noverplay".to_string();
                }
                AccountState::Restoring => {
                    self.status_message = "Сессия ещё проверяется".to_string();
                }
                AccountState::Authenticated { .. } => {
                    self.status_message = "Для выхода нажми x".to_string();
                }
            }
            return;
        }
        if self.screen == Screen::Settings {
            if let Some(kind) = CredentialKind::ALL.get(self.selected).copied() {
                self.modal = Some(Modal::Credential(Box::new(CredentialEditor::new(kind))));
                self.status_message = format!("Введи новый {}", kind.label());
            } else if self.selected == CredentialKind::ALL.len() {
                self.global_hotkeys_enabled = !self.global_hotkeys_enabled;
                self.config_dirty = true;
                self.status_message = "Настройка хоткеев применится после перезапуска".to_string();
            } else if let Some(action) = HotkeyAction::ALL
                .get(self.selected.saturating_sub(CredentialKind::ALL.len() + 1))
                .copied()
            {
                self.modal = Some(Modal::Hotkey(Box::new(HotkeyEditor {
                    action,
                    value: action.value(&self.hotkeys).to_string(),
                })));
                self.status_message = format!("Введи комбинацию для {}", action.label());
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
            Some(Modal::Account(dialog)) => match dialog.stage {
                AccountDialogStage::Credentials => {
                    if let Err(error) = dialog.validate_credentials() {
                        dialog.error = Some(error.to_string());
                        self.status_message = error.to_string();
                        return;
                    }
                    dialog.stage = AccountDialogStage::LoadingCaptcha;
                    dialog.error = None;
                    self.status_message = "Загружаем CAPTCHA".to_string();
                    self.effects
                        .push(AppEffect::LoadAccountCaptcha(dialog.action));
                }
                AccountDialogStage::Captcha => {
                    let solution = match dialog.solution() {
                        Ok(solution) => solution,
                        Err(error) => {
                            dialog.error = Some(error.to_string());
                            self.status_message = error.to_string();
                            return;
                        }
                    };
                    dialog.stage = AccountDialogStage::Submitting;
                    dialog.error = None;
                    self.status_message = match dialog.action {
                        AccountAction::Login => "Входим в Noverplay".to_string(),
                        AccountAction::Register => "Создаём аккаунт Noverplay".to_string(),
                    };
                    self.effects.push(AppEffect::AuthenticateAccount {
                        action: dialog.action,
                        username: dialog.username.clone(),
                        password: dialog.password.clone(),
                        captcha_id: dialog.captcha_id().to_string(),
                        solution,
                    });
                }
                AccountDialogStage::LoadingCaptcha | AccountDialogStage::Submitting => {}
            },
            Some(Modal::CommandPalette(state)) => {
                let command = state.command();
                self.modal = None;
                self.run_palette_command(command);
            }
            Some(Modal::Hotkey(editor)) => {
                if crate::hotkeys::validate_binding(&editor.value).is_err() {
                    self.status_message =
                        "Формат: Ctrl+Alt+P, Ctrl+Alt+Space или Ctrl+Alt+Right".to_string();
                    return;
                }
                editor
                    .action
                    .set(&mut self.hotkeys, editor.value.trim().to_string());
                self.config_dirty = true;
                self.modal = None;
                self.status_message = "Хоткей сохранён; применится после перезапуска".to_string();
            }
            _ => self.close_modal(),
        }
    }

    fn move_modal_selection(&mut self, next: bool) {
        if let Some(Modal::CommandPalette(state)) = self.modal.as_mut() {
            if next {
                state.next();
            } else {
                state.previous();
            }
            return;
        }
        if let Some(Modal::Account(dialog)) = self.modal.as_mut() {
            if next {
                dialog.select_next();
            } else {
                dialog.select_previous();
            }
            return;
        }
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
        if let Some(Modal::Account(dialog)) = self.modal.as_mut() {
            dialog.toggle_action();
            return;
        }
        self.update_onboarding(|state| {
            state.toggle();
            OnboardingCommand::None
        });
    }

    fn run_palette_command(&mut self, command: PaletteCommand) {
        match command {
            PaletteCommand::Search => {
                self.screen = Screen::Search;
                self.selected = 0;
            }
            PaletteCommand::ImportPlaylist => {
                self.modal = Some(Modal::PlaylistImport(Box::default()));
                self.status_message =
                    "Вставь ссылку на плейлист SoundCloud, Yandex или Deezer".to_string();
            }
            PaletteCommand::Profile => {
                self.screen = Screen::Profile;
                self.selected = 0;
            }
            PaletteCommand::Settings => {
                self.screen = Screen::Settings;
                self.selected = 0;
            }
            PaletteCommand::ProbeSoundCloud => {
                self.status_message = "Проверяем доступ к SoundCloud".to_string();
                self.effects.push(AppEffect::ProbeSoundCloud);
            }
        }
    }

    fn input_modal(&mut self, value: char) {
        match self.modal.as_mut() {
            Some(Modal::Onboarding(state)) => state.input(value),
            Some(Modal::Credential(editor)) => editor.input(value),
            Some(Modal::PlaylistImport(editor)) => editor.input(value),
            Some(Modal::Account(dialog)) => dialog.input(value),
            Some(Modal::Hotkey(editor)) => editor.input(value),
            _ => {}
        }
    }

    fn backspace_modal(&mut self) {
        match self.modal.as_mut() {
            Some(Modal::Onboarding(state)) => state.backspace(),
            Some(Modal::Credential(editor)) => editor.backspace(),
            Some(Modal::PlaylistImport(editor)) => editor.backspace(),
            Some(Modal::Account(dialog)) => dialog.backspace(),
            Some(Modal::Hotkey(editor)) => editor.backspace(),
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
                    CredentialKind::DeezerArl => self.deezer_enabled = true,
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
                self.active_playlist = None;
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

    fn toggle_like(&mut self) {
        let Some(track) = self.selected_tracks().get(self.selected).cloned() else {
            return;
        };
        let key = track.provider_key();
        let liked = !self
            .library
            .iter()
            .any(|current| current.provider_key() == key);
        if liked {
            self.library.insert(0, track.clone());
            self.status_message = format!("Добавили «{}» в библиотеку", track.title);
        } else {
            self.library.retain(|current| current.provider_key() != key);
            self.selected = self.selected.min(self.item_count().saturating_sub(1));
            self.status_message = format!("Убрали «{}» из библиотеки", track.title);
        }
        self.effects.push(AppEffect::SetLiked {
            track: Box::new(track),
            liked,
        });
    }

    fn finish_like(&mut self, track: TrackRef, liked: bool, result: Result<(), String>) {
        let Err(error) = result else {
            return;
        };
        let key = track.provider_key();
        if liked {
            self.library.retain(|current| current.provider_key() != key);
        } else if !self
            .library
            .iter()
            .any(|current| current.provider_key() == key)
        {
            self.library.insert(0, track);
        }
        self.status_message = format!("Библиотека не сохранилась: {error}");
    }

    fn finish_account_captcha(
        &mut self,
        action: AccountAction,
        result: Result<CaptchaChallenge, AccountApiError>,
    ) {
        let Some(Modal::Account(dialog)) = self.modal.as_mut() else {
            return;
        };
        if dialog.action != action {
            return;
        }
        match result {
            Ok(challenge) => {
                let text_mode = challenge.captcha_kind == "text";
                match dialog.set_challenge(challenge) {
                    Ok(()) => {
                        self.status_message = if text_mode {
                            "Реши задачу и введи ответ".to_string()
                        } else {
                            "Нажми четыре иконки по порядку".to_string()
                        };
                    }
                    Err(error) => {
                        dialog.stage = AccountDialogStage::Credentials;
                        dialog.error = Some(error.to_string());
                        self.status_message = format!("CAPTCHA не открылась: {error}");
                    }
                }
            }
            Err(error) => {
                dialog.stage = AccountDialogStage::Credentials;
                dialog.error = Some(error.to_string());
                self.status_message = format!("CAPTCHA не загрузилась: {error}");
            }
        }
    }

    fn finish_authentication(&mut self, result: Result<AccountSession, AccountApiError>) {
        match result {
            Ok(session) => {
                let name = session.user.display_name.clone();
                self.account = AccountState::Authenticated {
                    user: session.user,
                    expires_at: session.expires_at,
                };
                self.modal = self.onboarding_resume.take().map(Modal::Onboarding);
                self.config_dirty = true;
                self.status_message = format!("Вход выполнен: {name}");
                self.schedule_bootstrap();
            }
            Err(error) => {
                if let Some(Modal::Account(dialog)) = self.modal.as_mut() {
                    if error.code == "CAPTCHA_INVALID" {
                        dialog.authentication_failed(error.to_string());
                    } else {
                        dialog.stage = AccountDialogStage::Credentials;
                        dialog.error = Some(error.to_string());
                    }
                }
                self.status_message = format!("Не удалось войти: {error}");
            }
        }
    }

    fn finish_account_restore(&mut self, result: Result<Option<AccountSession>, AccountApiError>) {
        match result {
            Ok(Some(session)) => {
                let name = session.user.display_name.clone();
                self.account = AccountState::Authenticated {
                    user: session.user,
                    expires_at: session.expires_at,
                };
                self.status_message = format!("Сессия восстановлена: {name}");
                self.config_dirty = true;
                self.schedule_bootstrap();
            }
            Ok(None) => {
                self.account = AccountState::Guest;
                self.bootstrap = BootstrapState::Unknown;
                self.status_message = "Гостевой режим".to_string();
                self.config_dirty = true;
            }
            Err(error) => {
                self.account = AccountState::Guest;
                self.bootstrap = BootstrapState::Unknown;
                self.status_message = format!("Сессию не удалось проверить: {error}");
            }
        }
    }

    fn finish_bootstrap(&mut self, result: Result<BootstrapUpdate, AccountApiError>) {
        match result {
            Ok(update) => {
                self.soundcloud_refresh_at_ms = Some(update.refresh_at_ms);
                self.bootstrap = BootstrapState::Ready {
                    refresh_at: Some(update.refresh_at),
                };
                self.credentials.soundcloud = true;
                self.soundcloud_enabled = true;
                self.config_dirty = true;
                self.status_message = "SoundCloud client_id получен из аккаунта".to_string();
            }
            Err(error) => {
                let retry_seconds = error.retry_after_seconds.unwrap_or(300).max(60);
                self.soundcloud_refresh_at_ms =
                    Some(now_ms().saturating_add((retry_seconds as i64).saturating_mul(1_000)));
                self.bootstrap = BootstrapState::Failed(error.to_string());
                self.config_dirty = true;
                self.status_message =
                    format!("Аккаунт вошёл, но SoundCloud ключ не получен: {error}");
            }
        }
    }

    fn finish_logout(&mut self, result: Result<(), AccountApiError>, soundcloud_configured: bool) {
        self.account = AccountState::Guest;
        self.bootstrap = BootstrapState::Unknown;
        self.soundcloud_refresh_at_ms = None;
        self.credentials.soundcloud = soundcloud_configured;
        self.modal = None;
        self.config_dirty = true;
        self.status_message = match result {
            Ok(()) => "Выход выполнен".to_string(),
            Err(error) => format!("Локально вышли, сервер не подтвердил: {error}"),
        };
    }

    fn toggle_account_mode(&mut self) {
        if let Some(Modal::Account(dialog)) = self.modal.as_mut() {
            dialog.toggle_action();
        }
    }

    fn logout_account(&mut self) {
        if matches!(self.account, AccountState::Authenticated { .. }) {
            self.status_message = "Выходим из Noverplay".to_string();
            self.effects.push(AppEffect::LogoutAccount);
        }
    }

    fn schedule_bootstrap(&mut self) {
        if self
            .soundcloud_refresh_at_ms
            .is_some_and(|deadline| deadline > now_ms())
        {
            if !matches!(self.bootstrap, BootstrapState::Ready { .. }) {
                self.bootstrap = BootstrapState::Ready { refresh_at: None };
            }
            return;
        }
        self.bootstrap = BootstrapState::Refreshing;
        self.status_message = "Получаем SoundCloud client_id из аккаунта".to_string();
        self.effects.push(AppEffect::RefreshBootstrap);
    }

    fn click_modal(&mut self, column: u16, row: u16, terminal_width: u16, terminal_height: u16) {
        let Some(Modal::Account(dialog)) = self.modal.as_mut() else {
            return;
        };
        dialog.click(column, row, terminal_width, terminal_height);
        if dialog.stage == AccountDialogStage::Captcha {
            self.status_message = format!(
                "CAPTCHA: выбрано {}/{}",
                dialog.selected_clicks(),
                dialog.required_clicks()
            );
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
                if let Some(Modal::Onboarding(state)) = self.modal.take() {
                    self.onboarding_resume = Some(state);
                    self.modal = Some(Modal::Account(Box::new(AccountDialog::new(
                        AccountAction::Login,
                    ))));
                    self.status_message = "Введи логин и пароль Noverplay".to_string();
                }
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
        if matches!(self.modal, Some(Modal::Keybindings)) {
            self.keybindings_notice_seen = true;
            self.config_dirty = true;
        }
        if matches!(self.modal, Some(Modal::Account(_)))
            && let Some(mut state) = self.onboarding_resume.take()
        {
            state.account_mode = AccountMode::Guest;
            self.modal = Some(Modal::Onboarding(state));
            self.status_message = "Продолжаем быструю настройку в гостевом режиме".to_string();
            return;
        }
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

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use base64::{Engine, engine::general_purpose::STANDARD};

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
    fn command_palette_runs_the_selected_command_instead_of_being_a_poster() {
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
        app.handle(Action::OpenCommandPalette);
        for _ in 0..3 {
            app.handle(Action::ModalNext);
        }

        app.handle(Action::ModalSubmit);

        assert_eq!(app.screen, Screen::Settings);
        assert!(app.modal.is_none());
    }

    #[test]
    fn first_run_keybindings_notice_is_shown_once_and_acknowledged() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path().join("db.sqlite3"));
        storage.initialize().unwrap();
        let mut app = App::load(
            &storage,
            &AppConfig {
                onboarding_completed: true,
                keybindings_notice_seen: false,
                ..AppConfig::default()
            },
        )
        .unwrap();
        app.show_keybindings_notice_if_needed();
        assert!(matches!(app.modal, Some(Modal::Keybindings)));
        app.handle(Action::CloseModal);
        assert!(app.keybindings_notice_seen);
        assert!(app.config_dirty);
        app.show_keybindings_notice_if_needed();
        assert!(app.modal.is_none());
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
    fn control_wave_replaces_queue_and_starts_first_track() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path().join("db.sqlite3"));
        storage.initialize().unwrap();
        let mut app = App::load(&storage, &AppConfig::default()).unwrap();
        let first = test_track();
        let second = TrackRef {
            id: "second".to_string(),
            ..test_track()
        };

        assert_eq!(
            app.control_play_wave(vec![first.clone(), second.clone()]),
            Some((first.clone(), 2))
        );
        assert_eq!(app.queue, vec![first.clone(), second]);
        assert_eq!(app.queue_index, Some(0));
        assert_eq!(app.now_playing, Some(first.clone()));
        assert_eq!(app.take_effects(), vec![AppEffect::Play(Box::new(first))]);
    }

    #[test]
    fn control_queue_remove_keeps_current_index_valid() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path().join("db.sqlite3"));
        storage.initialize().unwrap();
        let mut app = App::load(&storage, &AppConfig::default()).unwrap();
        let first = test_track();
        let second = TrackRef {
            id: "second".to_string(),
            ..test_track()
        };
        let current = TrackRef {
            id: "current".to_string(),
            ..test_track()
        };
        app.queue = vec![first.clone(), second.clone(), current.clone()];
        app.queue_index = Some(2);
        app.now_playing = Some(current.clone());
        app.player.status = PlaybackStatus::Playing;

        assert_eq!(app.control_queue_remove(1).unwrap(), first);
        assert_eq!(app.queue_index, Some(1));
        assert_eq!(app.now_playing, Some(current.clone()));
        assert_eq!(app.control_queue_remove(2).unwrap(), current);
        assert_eq!(app.queue, vec![second]);
        assert_eq!(app.queue_index, None);
        assert_eq!(app.now_playing, None);
        assert_eq!(app.player.status, PlaybackStatus::Stopped);
        assert_eq!(app.take_effects(), vec![AppEffect::Stop]);
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
                provider: SearchProvider::All,
                immediate: true
            }]
        );
    }

    #[test]
    fn search_provider_is_part_of_the_effect_and_tab_cycle_resubmits() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path().join("db.sqlite3"));
        storage.initialize().unwrap();
        let mut app = App::load(&storage, &AppConfig::default()).unwrap();
        app.search_query = "ambient".to_string();
        app.handle(Action::CycleSearchProvider);
        assert_eq!(
            app.search_provider,
            crate::model::SearchProvider::SoundCloud
        );
        assert!(matches!(
            app.take_effects().as_slice(),
            [AppEffect::Search {
                provider: crate::model::SearchProvider::SoundCloud,
                ..
            }]
        ));
    }

    #[test]
    fn home_prefers_recent_history_and_library_is_still_the_full_favorites_list() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path().join("db.sqlite3"));
        storage.initialize().unwrap();
        let favorite = test_track();
        storage.like_track(&favorite, 1).unwrap();
        let recent = TrackRef {
            id: "recent".to_string(),
            ..test_track()
        };
        storage
            .record_history(&crate::storage::HistoryEntry {
                track: recent.clone(),
                played_at_ms: 2,
                completed: true,
                skipped: false,
            })
            .unwrap();
        let mut app = App::load(&storage, &AppConfig::default()).unwrap();
        assert_eq!(app.selected_tracks(), [recent]);
        app.handle(Action::Navigate(Screen::Library));
        assert_eq!(app.selected_tracks(), [favorite]);
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
    fn favorite_key_updates_library_and_asks_runtime_to_persist_it() {
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
        let track = test_track();
        app.screen = Screen::Search;
        app.search_results.push(track.clone());

        app.handle(Action::ToggleLike);

        assert_eq!(app.library, vec![track.clone()]);
        assert_eq!(
            app.take_effects(),
            vec![AppEffect::SetLiked {
                track: Box::new(track.clone()),
                liked: true,
            }]
        );
        app.handle(Action::LikeSaved {
            track: Box::new(track.clone()),
            liked: true,
            result: Ok(()),
        });
        app.handle(Action::ToggleLike);
        assert!(app.library.is_empty());
        assert_eq!(
            app.take_effects(),
            vec![AppEffect::SetLiked {
                track: Box::new(track),
                liked: false,
            }]
        );
    }

    #[test]
    fn profile_turns_credentials_and_human_clicks_into_a_login_request() {
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
        app.handle(Action::Navigate(Screen::Profile));
        app.handle(Action::Activate);
        for value in "User123".chars() {
            app.handle(Action::ModalInput(value));
        }
        app.handle(Action::ModalNext);
        for value in "password123".chars() {
            app.handle(Action::ModalInput(value));
        }
        app.handle(Action::ModalSubmit);
        assert_eq!(
            app.take_effects(),
            vec![AppEffect::LoadAccountCaptcha(AccountAction::Login)]
        );

        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="360" height="236"><rect width="360" height="236" fill="#ffffff"/></svg>"##;
        app.handle(Action::AccountCaptchaLoaded {
            action: AccountAction::Login,
            result: Ok(CaptchaChallenge {
                captcha_id: "captcha".to_string(),
                captcha_kind: "icon_sequence".to_string(),
                image_data_url: format!("data:image/svg+xml;base64,{}", STANDARD.encode(svg)),
                click_count_required: 4,
                action_type: "login".to_string(),
                expires_at: "later".to_string(),
                disabled: false,
                prompt: None,
            }),
        });
        let area = crate::account::captcha::captcha_cell_area(100, 32);
        for offset in 1..=4 {
            app.handle(Action::MouseClick {
                column: area.x + offset,
                row: area.y + 2,
                terminal_width: 100,
                terminal_height: 32,
            });
        }
        app.handle(Action::ModalSubmit);

        assert!(matches!(
            app.take_effects().as_slice(),
            [AppEffect::AuthenticateAccount {
                action: AccountAction::Login,
                username,
                password,
                captcha_id,
                solution,
            }] if username == "User123"
                && password == "password123"
                && captcha_id == "captcha"
                && solution.points.len() == 4
        ));
    }

    #[test]
    fn fresh_bootstrap_deadline_stops_the_client_from_hammering_the_server() {
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
        let session = AccountSession {
            user: crate::account::models::AccountUser {
                id: "1".to_string(),
                username: "user123".to_string(),
                display_name: "User".to_string(),
                uid: "42".to_string(),
                public_uid: "public".to_string(),
                created_at: "now".to_string(),
                avatar_url: String::new(),
                telemetry_opt_in: false,
            },
            expires_at: "later".to_string(),
        };

        app.handle(Action::AccountAuthenticated(Ok(session.clone())));
        assert_eq!(app.take_effects(), vec![AppEffect::RefreshBootstrap]);
        app.handle(Action::BootstrapFinished(Ok(BootstrapUpdate {
            protocol: "noverplay-app-auth-v2".to_string(),
            refresh_at: "later".to_string(),
            refresh_at_ms: now_ms() + 3_600_000,
        })));
        app.handle(Action::AccountAuthenticated(Ok(session)));

        assert!(app.take_effects().is_empty());
        assert!(matches!(app.bootstrap, BootstrapState::Ready { .. }));
    }

    #[test]
    fn first_run_login_returns_to_provider_setup_instead_of_finishing_the_wizard() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path().join("db.sqlite3"));
        storage.initialize().unwrap();
        let mut app = App::load(&storage, &AppConfig::default()).unwrap();

        app.handle(Action::ModalSubmit);
        app.handle(Action::ModalSubmit);

        assert!(matches!(app.modal, Some(Modal::Account(_))));
        assert!(app.onboarding_open());

        app.handle(Action::AccountAuthenticated(Ok(AccountSession {
            user: crate::account::models::AccountUser {
                id: "1".to_string(),
                username: "user123".to_string(),
                display_name: "User".to_string(),
                uid: "42".to_string(),
                public_uid: "public".to_string(),
                created_at: "now".to_string(),
                avatar_url: String::new(),
                telemetry_opt_in: false,
            },
            expires_at: "later".to_string(),
        })));

        assert!(matches!(
            app.modal,
            Some(Modal::Onboarding(ref state))
                if state.step == crate::onboarding::OnboardingStep::Providers
                    && state.account_mode == AccountMode::Account
        ));
        assert!(app.onboarding_open());
    }

    #[test]
    fn cancelling_first_run_login_continues_as_guest() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path().join("db.sqlite3"));
        storage.initialize().unwrap();
        let mut app = App::load(&storage, &AppConfig::default()).unwrap();
        app.handle(Action::ModalSubmit);
        app.handle(Action::ModalSubmit);

        app.handle(Action::CloseModal);

        assert!(matches!(
            app.modal,
            Some(Modal::Onboarding(ref state))
                if state.step == crate::onboarding::OnboardingStep::Providers
                    && state.account_mode == AccountMode::Guest
        ));
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

        app.handle(Action::Activate);
        assert!(app.active_playlist.is_some());
        assert_eq!(app.selected_tracks(), [test_track()]);
        app.handle(Action::Activate);
        assert!(matches!(
            app.take_effects().as_slice(),
            [AppEffect::Play(_)]
        ));
        app.handle(Action::Back);
        assert!(app.active_playlist.is_none());
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
            drm: false,
        }
    }
}
