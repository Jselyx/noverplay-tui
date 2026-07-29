mod message;
mod playback;
mod providers;
mod search;

use std::{sync::Arc, time::Duration};

use tokio::{sync::mpsc, task::JoinHandle};

use crate::{
    action::Action,
    audio::{AudioEngine, AudioStatus},
    config::AppConfig,
    effect::AppEffect,
    provider::ProviderRegistry,
    secrets::SecretStore,
};

use message::RuntimeMessage;
use playback::spawn_playback;
use providers::build_registry;
use search::spawn_search;

pub struct Runtime {
    providers: Arc<ProviderRegistry>,
    audio: Option<AudioEngine>,
    sender: mpsc::UnboundedSender<RuntimeMessage>,
    receiver: mpsc::UnboundedReceiver<RuntimeMessage>,
    search_task: Option<JoinHandle<()>>,
    playback_task: Option<JoinHandle<()>>,
    search_generation: u64,
    playback_generation: u64,
    search_delay: Duration,
    last_audio_status: Option<AudioStatus>,
    notices: Vec<String>,
}

impl Runtime {
    pub fn new(config: &AppConfig, secrets: &SecretStore) -> Self {
        let setup = build_registry(config, secrets);
        let mut notices = setup.notices;
        let audio = match AudioEngine::new(config.audio_output.as_deref(), config.volume_percent) {
            Ok(audio) => Some(audio),
            Err(error) => {
                notices.push(format!("Аудиовыход недоступен: {error}"));
                None
            }
        };
        let (sender, receiver) = mpsc::unbounded_channel();
        Self {
            providers: Arc::new(setup.registry),
            audio,
            sender,
            receiver,
            search_task: None,
            playback_task: None,
            search_generation: 0,
            playback_generation: 0,
            search_delay: Duration::from_millis(config.search_debounce_ms),
            last_audio_status: None,
            notices,
        }
    }

    pub fn take_notices(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notices)
    }

    pub fn dispatch(&mut self, effects: Vec<AppEffect>) -> Vec<Action> {
        let mut actions = Vec::new();
        for effect in effects {
            match effect {
                AppEffect::Search { query, immediate } => {
                    self.start_search(query, immediate, &mut actions)
                }
                AppEffect::Play(track) => self.start_playback(*track, &mut actions),
                AppEffect::Pause => {
                    if let Some(audio) = &self.audio {
                        audio.pause();
                    }
                }
                AppEffect::Resume => {
                    if let Some(audio) = &self.audio {
                        audio.resume();
                    }
                }
                AppEffect::Seek(position_ms) => {
                    if let Some(audio) = &self.audio
                        && let Err(error) = audio.seek_to(position_ms)
                    {
                        actions.push(Action::PlaybackFailed(error.to_string()));
                    }
                }
                AppEffect::SetVolume(volume) => {
                    if let Some(audio) = &self.audio {
                        audio.set_volume(volume);
                    }
                }
                AppEffect::Stop => {
                    self.cancel_playback();
                    if let Some(audio) = &self.audio {
                        audio.stop();
                    }
                }
            }
        }
        actions
    }

    pub fn poll_actions(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        while let Ok(message) = self.receiver.try_recv() {
            match message {
                RuntimeMessage::SearchFinished {
                    generation,
                    query,
                    tracks,
                    failures,
                } if generation == self.search_generation => {
                    actions.push(Action::SearchFinished {
                        query,
                        tracks,
                        failures,
                    });
                }
                RuntimeMessage::PlaybackReady { generation, source }
                    if generation == self.playback_generation =>
                {
                    if let Some(audio) = &self.audio {
                        audio.play(source);
                    } else {
                        actions.push(Action::PlaybackFailed(
                            "в системе не найден аудиовыход".to_string(),
                        ));
                    }
                }
                RuntimeMessage::PlaybackFailed { generation, error }
                    if generation == self.playback_generation =>
                {
                    actions.push(Action::PlaybackFailed(error));
                }
                _ => {}
            }
        }
        if let Some(audio) = &self.audio {
            while let Some(event) = audio.try_event() {
                actions.push(Action::Audio(event));
            }
            let status = audio.status();
            if self.last_audio_status.as_ref() != Some(&status) {
                actions.push(Action::AudioProgress {
                    position_ms: status.position_ms,
                    buffered_ms: status.buffered_ms,
                });
                self.last_audio_status = Some(status);
            }
        }
        actions
    }

    fn start_search(&mut self, query: String, immediate: bool, actions: &mut Vec<Action>) {
        if let Some(task) = self.search_task.take() {
            task.abort();
        }
        self.search_generation = self.search_generation.wrapping_add(1);
        if query.is_empty() {
            actions.push(Action::SearchFinished {
                query,
                tracks: Vec::new(),
                failures: Vec::new(),
            });
            return;
        }
        let delay = if immediate {
            Duration::ZERO
        } else {
            self.search_delay
        };
        self.search_task = Some(spawn_search(
            Arc::clone(&self.providers),
            self.sender.clone(),
            self.search_generation,
            query,
            delay,
        ));
    }

    fn start_playback(&mut self, track: crate::model::TrackRef, actions: &mut Vec<Action>) {
        self.cancel_playback();
        let Some(provider) = self.providers.get(track.provider) else {
            actions.push(Action::PlaybackFailed(format!(
                "{} не настроен",
                track.provider.label()
            )));
            return;
        };
        if let Some(audio) = &self.audio {
            audio.reset();
        }
        self.playback_task = Some(spawn_playback(
            provider,
            self.sender.clone(),
            self.playback_generation,
            track,
        ));
    }

    fn cancel_playback(&mut self) {
        if let Some(task) = self.playback_task.take() {
            task.abort();
        }
        self.playback_generation = self.playback_generation.wrapping_add(1);
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        if let Some(task) = self.search_task.take() {
            task.abort();
        }
        if let Some(task) = self.playback_task.take() {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_runtime_message_never_reaches_app() {
        let (sender, receiver) = mpsc::unbounded_channel();
        let mut runtime = Runtime {
            providers: Arc::new(ProviderRegistry::default()),
            audio: None,
            sender: sender.clone(),
            receiver,
            search_task: None,
            playback_task: None,
            search_generation: 7,
            playback_generation: 3,
            search_delay: Duration::ZERO,
            last_audio_status: None,
            notices: Vec::new(),
        };
        sender
            .send(RuntimeMessage::SearchFinished {
                generation: 6,
                query: "старьё".to_string(),
                tracks: Vec::new(),
                failures: Vec::new(),
            })
            .unwrap();
        sender
            .send(RuntimeMessage::PlaybackFailed {
                generation: 2,
                error: "старьё".to_string(),
            })
            .unwrap();
        assert!(runtime.poll_actions().is_empty());
    }

    #[test]
    fn empty_search_cancels_task_and_clears_results_immediately() {
        let (sender, receiver) = mpsc::unbounded_channel();
        let mut runtime = Runtime {
            providers: Arc::new(ProviderRegistry::default()),
            audio: None,
            sender,
            receiver,
            search_task: None,
            playback_task: None,
            search_generation: 0,
            playback_generation: 0,
            search_delay: Duration::from_secs(1),
            last_audio_status: None,
            notices: Vec::new(),
        };
        let actions = runtime.dispatch(vec![AppEffect::Search {
            query: String::new(),
            immediate: false,
        }]);
        assert!(matches!(
            actions.as_slice(),
            [Action::SearchFinished { query, tracks, failures }]
                if query.is_empty() && tracks.is_empty() && failures.is_empty()
        ));
    }
}
