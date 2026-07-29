use std::{
    collections::BTreeMap,
    io::{self, Read, Seek, SeekFrom},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
    thread,
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use cpal::{
    Device, FromSample, I24, OutputCallbackInfo, SampleFormat, SizedSample, Stream, StreamConfig,
    U24,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use crossbeam_channel::{Receiver, Sender, TryRecvError, bounded, unbounded};
use reqwest::{
    StatusCode,
    blocking::Client,
    header::{CONTENT_LENGTH, CONTENT_RANGE, HeaderMap, HeaderName, HeaderValue, RANGE},
};
use symphonia::core::{
    codecs::audio::AudioDecoderOptions,
    errors::Error as SymphoniaError,
    formats::{FormatOptions, SeekMode, SeekTo, TrackType, probe::Hint},
    io::{MediaSource, MediaSourceStream, MediaSourceStreamOptions},
    meta::MetadataOptions,
    units::Time,
};

use crate::model::PlaybackSource;

const HTTP_CHUNK_BYTES: usize = 256 * 1024;
const AUDIO_CHUNKS: usize = 12;

#[derive(Clone, Debug, PartialEq)]
pub enum AudioEvent {
    Buffering,
    Playing,
    Paused,
    Stopped,
    Ended,
    Failed(String),
    OutputFailed(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct AudioStatus {
    pub position_ms: u64,
    pub buffered_ms: u64,
    pub paused: bool,
    pub volume_percent: u8,
    pub output_name: String,
}

struct AudioChunk {
    generation: u64,
    samples: Vec<f32>,
    end: bool,
}

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
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.played_samples.store(0, Ordering::Release);
        self.buffered_samples.store(0, Ordering::Release);
        if let Ok(mut current) = self.source.lock() {
            *current = None;
        }
        let _ = self.event_tx.send(AudioEvent::Stopped);
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

#[allow(clippy::too_many_arguments)]
fn build_stream(
    device: &Device,
    config: &StreamConfig,
    format: SampleFormat,
    chunks: Receiver<AudioChunk>,
    events: Sender<AudioEvent>,
    generation: Arc<AtomicU64>,
    paused: Arc<AtomicBool>,
    volume: Arc<AtomicU32>,
    played_samples: Arc<AtomicU64>,
    buffered_samples: Arc<AtomicU64>,
) -> Result<Stream> {
    macro_rules! typed_stream {
        ($sample:ty) => {
            sobrat_stream_dlya_tipa::<$sample>(
                device,
                config,
                chunks,
                events,
                generation,
                paused,
                volume,
                played_samples,
                buffered_samples,
            )
        };
    }

    match format {
        SampleFormat::I8 => typed_stream!(i8),
        SampleFormat::I16 => typed_stream!(i16),
        SampleFormat::I24 => typed_stream!(I24),
        SampleFormat::I32 => typed_stream!(i32),
        SampleFormat::I64 => typed_stream!(i64),
        SampleFormat::U8 => typed_stream!(u8),
        SampleFormat::U16 => typed_stream!(u16),
        SampleFormat::U24 => typed_stream!(U24),
        SampleFormat::U32 => typed_stream!(u32),
        SampleFormat::U64 => typed_stream!(u64),
        SampleFormat::F32 => typed_stream!(f32),
        SampleFormat::F64 => typed_stream!(f64),
        _ => bail!("формат аудиовыхода {format} не поддерживается"),
    }
}

#[allow(clippy::too_many_arguments)]
fn sobrat_stream_dlya_tipa<T>(
    device: &Device,
    config: &StreamConfig,
    chunks: Receiver<AudioChunk>,
    events: Sender<AudioEvent>,
    generation: Arc<AtomicU64>,
    paused: Arc<AtomicBool>,
    volume: Arc<AtomicU32>,
    played_samples: Arc<AtomicU64>,
    buffered_samples: Arc<AtomicU64>,
) -> Result<Stream>
where
    T: SizedSample + FromSample<f32>,
{
    let error_events = events.clone();
    let mut current = Vec::new();
    let mut cursor = 0usize;
    let mut chunk_generation = 0u64;
    let mut audible = false;
    let stream = device.build_output_stream(
        *config,
        move |output: &mut [T], _: &OutputCallbackInfo| {
            let active_generation = generation.load(Ordering::Acquire);
            if chunk_generation != active_generation {
                current.clear();
                cursor = 0;
                chunk_generation = active_generation;
                audible = false;
            }
            if paused.load(Ordering::Acquire) {
                output.fill(T::from_sample(0.0));
                return;
            }
            let mut consumed = 0u64;
            for target in output.iter_mut() {
                loop {
                    if cursor < current.len() {
                        let value = current[cursor];
                        cursor += 1;
                        *target =
                            T::from_sample(value * volume.load(Ordering::Relaxed) as f32 / 100.0);
                        consumed += 1;
                        if !audible {
                            audible = true;
                            let _ = events.send(AudioEvent::Playing);
                        }
                        break;
                    }
                    match chunks.try_recv() {
                        Ok(chunk) if chunk.generation != active_generation => continue,
                        Ok(chunk) if chunk.end => {
                            let _ = events.send(AudioEvent::Ended);
                            audible = false;
                            *target = T::from_sample(0.0);
                            break;
                        }
                        Ok(chunk) => {
                            chunk_generation = chunk.generation;
                            current = chunk.samples;
                            cursor = 0;
                        }
                        Err(TryRecvError::Empty) => {
                            if audible {
                                audible = false;
                                let _ = events.send(AudioEvent::Buffering);
                            }
                            *target = T::from_sample(0.0);
                            break;
                        }
                        Err(TryRecvError::Disconnected) => {
                            *target = T::from_sample(0.0);
                            break;
                        }
                    }
                }
            }
            played_samples.fetch_add(consumed, Ordering::Relaxed);
            let _ =
                buffered_samples.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |buffered| {
                    Some(buffered.saturating_sub(consumed))
                });
        },
        move |error| {
            let _ = error_events.send(AudioEvent::OutputFailed(error.to_string()));
        },
        None,
    )?;
    Ok(stream)
}

#[allow(clippy::too_many_arguments)]
fn decode_source(
    source: PlaybackSource,
    position_ms: u64,
    generation: u64,
    current_generation: &AtomicU64,
    output_rate: u32,
    output_channels: usize,
    chunks: &Sender<AudioChunk>,
    buffered_samples: &AtomicU64,
) -> Result<()> {
    let media = HttpRangeSource::open(source.url.as_str(), &source.headers, source.supports_range)?;
    let stream = MediaSourceStream::new(
        Box::new(media),
        MediaSourceStreamOptions {
            buffer_len: 64 * 1024,
        },
    );
    let mut hint = Hint::new();
    if let Some(extension) = source
        .url
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .and_then(|name| name.rsplit_once('.').map(|(_, extension)| extension))
    {
        hint.with_extension(extension);
    }
    if let Some(mime) = source.mime_type.as_deref() {
        hint.mime_type(mime);
    }
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .context("формат аудиопотока не распознан")?;
    let track = format
        .default_track(TrackType::Audio)
        .context("в источнике нет аудиодорожки")?
        .clone();
    let codec = track
        .codec_params
        .as_ref()
        .and_then(|params| params.audio())
        .context("у аудиодорожки нет параметров кодека")?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(codec, &AudioDecoderOptions::default())
        .context("аудиокодек не поддерживается")?;

    if position_ms > 0 {
        let seconds = (position_ms / 1000) as i64;
        let nanos = ((position_ms % 1000) * 1_000_000) as u32;
        let time = Time::try_new(seconds, nanos).context("позиция перемотки слишком большая")?;
        format
            .seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time,
                    track_id: Some(track.id),
                },
            )
            .context("источник не поддерживает перемотку")?;
        decoder.reset();
    }

    while current_generation.load(Ordering::Acquire) == generation {
        let Some(packet) = format
            .next_packet()
            .context("не удалось прочитать аудиопакет")?
        else {
            break;
        };
        if packet.track_id != track.id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(SymphoniaError::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(error) => return Err(error).context("не удалось декодировать аудиопакет"),
        };
        let input_channels = decoded.spec().channels().count();
        let input_rate = decoded.spec().rate();
        let mut samples = vec![0.0; decoded.samples_interleaved()];
        decoded.copy_to_slice_interleaved(&mut samples);
        let samples = convert_audio(
            &samples,
            input_channels,
            input_rate,
            output_channels,
            output_rate,
        );
        let sample_count = samples.len() as u64;
        buffered_samples.fetch_add(sample_count, Ordering::AcqRel);
        if chunks
            .send(AudioChunk {
                generation,
                samples,
                end: false,
            })
            .is_err()
        {
            break;
        }
    }
    Ok(())
}

fn convert_audio(
    input: &[f32],
    input_channels: usize,
    input_rate: u32,
    output_channels: usize,
    output_rate: u32,
) -> Vec<f32> {
    if input.is_empty() || input_channels == 0 || output_channels == 0 || input_rate == 0 {
        return Vec::new();
    }
    let input_frames = input.len() / input_channels;
    let output_frames =
        ((input_frames as u64 * u64::from(output_rate)) / u64::from(input_rate)).max(1) as usize;
    let mut output = Vec::with_capacity(output_frames * output_channels);

    // Линейная интерполяция тут не Нобелевка, зато не тащит DSP-комбайн весом с тюремный барак 🤡
    for output_frame in 0..output_frames {
        let position = output_frame as f64 * input_rate as f64 / output_rate as f64;
        let left = (position.floor() as usize).min(input_frames - 1);
        let right = (left + 1).min(input_frames - 1);
        let fraction = (position - left as f64) as f32;
        for output_channel in 0..output_channels {
            let sample = if input_channels == 1 {
                input[left] * (1.0 - fraction) + input[right] * fraction
            } else if output_channels == 1 {
                let left_average = average_frame(input, left, input_channels);
                let right_average = average_frame(input, right, input_channels);
                left_average * (1.0 - fraction) + right_average * fraction
            } else {
                let input_channel = output_channel.min(input_channels - 1);
                let left_sample = input[left * input_channels + input_channel];
                let right_sample = input[right * input_channels + input_channel];
                left_sample * (1.0 - fraction) + right_sample * fraction
            };
            output.push(sample);
        }
    }
    output
}

fn average_frame(input: &[f32], frame: usize, channels: usize) -> f32 {
    let start = frame * channels;
    input[start..start + channels].iter().sum::<f32>() / channels as f32
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

struct HttpRangeSource {
    client: Client,
    url: String,
    headers: HeaderMap,
    position: u64,
    length: Option<u64>,
    range_supported: bool,
    chunk_start: u64,
    chunk: Vec<u8>,
}

impl HttpRangeSource {
    fn open(url: &str, headers: &BTreeMap<String, String>, prefer_range: bool) -> Result<Self> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()?;
        let headers = normalize_headers(headers)?;
        let mut source = Self {
            client,
            url: url.to_string(),
            headers,
            position: 0,
            length: None,
            range_supported: prefer_range,
            chunk_start: 0,
            chunk: Vec::new(),
        };
        source.fetch(0)?;
        Ok(source)
    }

    fn fetch(&mut self, start: u64) -> io::Result<()> {
        let mut last_error = None;
        for attempt in 0..3 {
            match self.fetch_once(start) {
                Ok(()) => return Ok(()),
                Err(error) => {
                    last_error = Some(error);
                    if attempt < 2 {
                        thread::sleep(Duration::from_millis(150 * (attempt + 1)));
                    }
                }
            }
        }
        Err(last_error.unwrap_or_else(|| io::Error::other("источник не ответил")))
    }

    fn fetch_once(&mut self, start: u64) -> io::Result<()> {
        let end = start.saturating_add(HTTP_CHUNK_BYTES as u64 - 1);
        let mut request = self.client.get(&self.url).headers(self.headers.clone());
        if self.range_supported || start > 0 {
            request = request.header(RANGE, format!("bytes={start}-{end}"));
        }
        let response = request.send().map_err(io_error)?;
        let status = response.status();
        if !status.is_success() {
            return Err(io::Error::other(format!("источник вернул HTTP {status}")));
        }
        let headers = response.headers().clone();
        let partial = status == StatusCode::PARTIAL_CONTENT;
        if start > 0 && !partial {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "сервер игнорирует HTTP Range",
            ));
        }
        self.range_supported = partial;
        self.length = dlina_iz_range(&headers).or_else(|| {
            if start == 0 && !partial {
                headers
                    .get(CONTENT_LENGTH)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse().ok())
            } else {
                self.length
            }
        });
        let mut chunk = Vec::with_capacity(HTTP_CHUNK_BYTES);
        response
            .take(HTTP_CHUNK_BYTES as u64)
            .read_to_end(&mut chunk)
            .map_err(io_error)?;
        self.chunk_start = start;
        self.chunk = chunk;
        Ok(())
    }
}

impl Read for HttpRangeSource {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() || self.length.is_some_and(|length| self.position >= length) {
            return Ok(0);
        }
        let chunk_end = self.chunk_start + self.chunk.len() as u64;
        if self.position < self.chunk_start || self.position >= chunk_end {
            self.fetch(self.position)?;
            if self.chunk.is_empty() {
                return Ok(0);
            }
        }
        let offset = (self.position - self.chunk_start) as usize;
        let count = output.len().min(self.chunk.len() - offset);
        output[..count].copy_from_slice(&self.chunk[offset..offset + count]);
        self.position += count as u64;
        Ok(count)
    }
}

impl Seek for HttpRangeSource {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let target = match from {
            SeekFrom::Start(position) => position as i128,
            SeekFrom::Current(offset) => self.position as i128 + offset as i128,
            SeekFrom::End(offset) => {
                self.length
                    .ok_or_else(|| io::Error::new(io::ErrorKind::Unsupported, "длина неизвестна"))?
                    as i128
                    + offset as i128
            }
        };
        if target < 0 || target > u64::MAX as i128 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "позиция вне источника",
            ));
        }
        let target = target as u64;
        if !self.range_supported && target != self.position {
            // Сервер сказал Range? НЕ ЗНАЮ ТАКОГО, сиди теперь без перемотки как честный арестант ✌️
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "источник не поддерживает HTTP Range",
            ));
        }
        self.position = target;
        Ok(target)
    }
}

impl MediaSource for HttpRangeSource {
    fn is_seekable(&self) -> bool {
        self.range_supported
    }

    fn byte_len(&self) -> Option<u64> {
        self.length
    }
}

fn normalize_headers(headers: &BTreeMap<String, String>) -> Result<HeaderMap> {
    let mut parsed = HeaderMap::new();
    for (name, value) in headers {
        parsed.insert(
            HeaderName::from_bytes(name.as_bytes()).context("неверное имя HTTP-заголовка")?,
            HeaderValue::from_str(value).context("неверное значение HTTP-заголовка")?,
        );
    }
    Ok(parsed)
}

fn dlina_iz_range(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(CONTENT_RANGE)?
        .to_str()
        .ok()?
        .rsplit_once('/')?
        .1
        .parse()
        .ok()
}

fn io_error(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_is_duplicated_for_stereo() {
        let output = convert_audio(&[0.25, 0.5], 1, 48_000, 2, 48_000);
        assert_eq!(output, vec![0.25, 0.25, 0.5, 0.5]);
    }

    #[test]
    fn stereo_is_mixed_to_mono() {
        let output = convert_audio(&[1.0, -1.0, 0.5, 0.5], 2, 44_100, 1, 44_100);
        assert_eq!(output, vec![0.0, 0.5]);
    }

    #[test]
    fn resampler_changes_frame_count() {
        let input = vec![0.0; 2 * 24_000];
        let output = convert_audio(&input, 2, 24_000, 2, 48_000);
        assert_eq!(output.len(), 2 * 48_000);
    }

    #[test]
    fn content_range_yields_total_length() {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_RANGE, HeaderValue::from_static("bytes 0-255/12345"));
        assert_eq!(dlina_iz_range(&headers), Some(12_345));
    }
}
