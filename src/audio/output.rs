use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
};

use anyhow::{Result, bail};
use cpal::{
    Device, FromSample, I24, OutputCallbackInfo, SampleFormat, SizedSample, Stream, StreamConfig,
    U24, traits::DeviceTrait,
};
use crossbeam_channel::{Receiver, Sender, TryRecvError};

use super::types::AudioEvent;

pub(super) struct AudioChunk {
    pub(super) generation: u64,
    pub(super) samples: Vec<f32>,
    pub(super) end: bool,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build_stream(
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
