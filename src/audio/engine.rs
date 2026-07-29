use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
    thread,
};

use anyhow::{Context, Result, anyhow};
use cpal::{
    Device, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use crossbeam_channel::{Receiver, Sender, bounded, unbounded};

use crate::model::PlaybackSource;

use super::{
    decoder::decode_source,
    output::{AudioChunk, build_stream},
    types::{AudioEvent, AudioStatus},
};

const AUDIO_CHUNKS: usize = 12;

pub struct AudioEngine {
    _stream: Stream,
    source: Arc<Mutex<Option<PlaybackSource>>>,
    chunks: Sender<AudioChunk>,
    events: Receiver<AudioEvent>,
    event_tx: Sender<AudioEvent>,
    generation: Arc<AtomicU64>,
    paused: Arc<AtomicBool>,
    volume: Arc<AtomicU32>,
    played_samples: Arc<AtomicU64>,
    buffered_samples: Arc<AtomicU64>,
    output_rate: u32,
    output_channels: usize,
    output_name: String,
}

impl AudioEngine {
    pub fn new(preferred_output: Option<&str>, volume_percent: u8) -> Result<Self> {
        let host = cpal::default_host();
        let device = select_output(&host, preferred_output)?;
        let output_name = device
            .description()
            .map(|description| description.name().to_string())
            .unwrap_or_else(|_| "Системный аудиовыход".to_string());
        let supported = device
            .default_output_config()
            .context("не удалось получить формат аудиовыхода")?;
        let sample_format = supported.sample_format();
        let config: StreamConfig = supported.into();
        let output_rate = config.sample_rate;
        let output_channels = usize::from(config.channels);
        let (chunk_tx, chunk_rx) = bounded(AUDIO_CHUNKS);
        let (event_tx, event_rx) = unbounded();
        let generation = Arc::new(AtomicU64::new(0));
        let paused = Arc::new(AtomicBool::new(false));
        let volume = Arc::new(AtomicU32::new(volume_percent.min(100) as u32));
        let played_samples = Arc::new(AtomicU64::new(0));
        let buffered_samples = Arc::new(AtomicU64::new(0));

        // В колбэке нельзя устраивать сходку с мьютексами, аудиодрайвер за такое этапирует звук в лагерь 🫩
        let stream = build_stream(
            &device,
            &config,
            sample_format,
            chunk_rx,
            event_tx.clone(),
            Arc::clone(&generation),
            Arc::clone(&paused),
            Arc::clone(&volume),
            Arc::clone(&played_samples),
            Arc::clone(&buffered_samples),
        )?;
        stream.play().context("не удалось запустить аудиовыход")?;

        Ok(Self {
            _stream: stream,
            source: Arc::new(Mutex::new(None)),
            chunks: chunk_tx,
            events: event_rx,
            event_tx,
            generation,
            paused,
            volume,
            played_samples,
            buffered_samples,
            output_rate,
            output_channels,
            output_name,
        })
    }

    pub fn output_devices() -> Result<Vec<String>> {
        let host = cpal::default_host();
        let mut names = host
            .output_devices()
            .context("не удалось получить аудиовыходы")?
            .filter_map(|device| {
                device
                    .description()
                    .ok()
                    .map(|value| value.name().to_string())
            })
            .collect::<Vec<_>>();
        names.sort_unstable();
        names.dedup();
        Ok(names)
    }

    pub fn play(&self, source: PlaybackSource) {
        if let Ok(mut current) = self.source.lock() {
            *current = Some(source.clone());
        }
        self.start_decoder(source, 0);
    }

    pub fn seek_to(&self, position_ms: u64) -> Result<()> {
        let source = self
            .source
            .lock()
            .map_err(|_| anyhow!("состояние аудио повреждено"))?
            .clone()
            .context("сейчас ничего не воспроизводится")?;
        self.start_decoder(source, position_ms);
        Ok(())
    }

    pub fn pause(&self) {
        self.paused.store(true, Ordering::Release);
        let _ = self.event_tx.send(AudioEvent::Paused);
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::Release);
        let _ = self.event_tx.send(AudioEvent::Playing);
    }

    pub fn toggle_pause(&self) {
        if self.paused.load(Ordering::Acquire) {
            self.resume();
        } else {
            self.pause();
        }
    }

    pub fn stop(&self) {
        self.reset();
        let _ = self.event_tx.send(AudioEvent::Stopped);
    }

    pub fn reset(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.played_samples.store(0, Ordering::Release);
        self.buffered_samples.store(0, Ordering::Release);
        if let Ok(mut current) = self.source.lock() {
            *current = None;
        }
    }

    pub fn set_volume(&self, volume_percent: u8) {
        self.volume
            .store(volume_percent.min(100) as u32, Ordering::Release);
    }

    pub fn status(&self) -> AudioStatus {
        let samples_per_second = u64::from(self.output_rate) * self.output_channels as u64;
        AudioStatus {
            position_ms: samples_to_ms(
                self.played_samples.load(Ordering::Acquire),
                samples_per_second,
            ),
            buffered_ms: samples_to_ms(
                self.buffered_samples.load(Ordering::Acquire),
                samples_per_second,
            ),
            paused: self.paused.load(Ordering::Acquire),
            volume_percent: self.volume.load(Ordering::Acquire) as u8,
            output_name: self.output_name.clone(),
        }
    }

    pub fn try_event(&self) -> Option<AudioEvent> {
        self.events.try_recv().ok()
    }

    fn start_decoder(&self, source: PlaybackSource, position_ms: u64) {
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.played_samples.store(
            position_ms
                .saturating_mul(u64::from(self.output_rate))
                .saturating_mul(self.output_channels as u64)
                / 1000,
            Ordering::Release,
        );
        self.buffered_samples.store(0, Ordering::Release);
        self.paused.store(false, Ordering::Release);
        let chunks = self.chunks.clone();
        let events = self.event_tx.clone();
        let current_generation = Arc::clone(&self.generation);
        let buffered_samples = Arc::clone(&self.buffered_samples);
        let output_rate = self.output_rate;
        let output_channels = self.output_channels;
        let _ = events.send(AudioEvent::Buffering);

        thread::spawn(move || {
            let result = decode_source(
                source,
                position_ms,
                generation,
                &current_generation,
                output_rate,
                output_channels,
                &chunks,
                &buffered_samples,
            );
            if current_generation.load(Ordering::Acquire) != generation {
                return;
            }
            match result {
                Ok(()) => {
                    // Последняя пустая посылка это конвойный с табличкой КОНЕЦ, не стреляем в пианиста))))
                    let _ = chunks.send(AudioChunk {
                        generation,
                        samples: Vec::new(),
                        end: true,
                    });
                }
                Err(error) => {
                    let _ = events.send(AudioEvent::Failed(error.to_string()));
                }
            }
        });
    }
}

fn samples_to_ms(samples: u64, samples_per_second: u64) -> u64 {
    if samples_per_second == 0 {
        0
    } else {
        samples.saturating_mul(1000) / samples_per_second
    }
}

fn select_output(host: &cpal::Host, preferred: Option<&str>) -> Result<Device> {
    if let Some(preferred) = preferred.filter(|name| !name.trim().is_empty())
        && let Some(device) = host
            .output_devices()
            .context("не удалось получить аудиовыходы")?
            .find(|device| {
                device
                    .description()
                    .is_ok_and(|description| description.name() == preferred)
            })
    {
        return Ok(device);
    }
    host.default_output_device()
        .context("в системе не найден аудиовыход")
}
