use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};
use crossbeam_channel::Sender;
use symphonia::core::{
    codecs::audio::AudioDecoderOptions,
    errors::Error as SymphoniaError,
    formats::{FormatOptions, SeekMode, SeekTo, TrackType, probe::Hint},
    io::{MediaSourceStream, MediaSourceStreamOptions},
    meta::MetadataOptions,
    units::Time,
};

use crate::model::PlaybackSource;

use super::{convert::convert_audio, media::open_media, output::AudioChunk};

#[allow(clippy::too_many_arguments)]
pub(super) fn decode_source(
    source: PlaybackSource,
    position_ms: u64,
    generation: u64,
    current_generation: &AtomicU64,
    output_rate: u32,
    output_channels: usize,
    chunks: &Sender<AudioChunk>,
    buffered_samples: &AtomicU64,
) -> Result<()> {
    let media = open_media(&source, position_ms)?;
    let stream = MediaSourceStream::new(
        media.source,
        MediaSourceStreamOptions {
            buffer_len: 64 * 1024,
        },
    );
    let mut hint = Hint::new();
    if let Some(extension) = media.extension.as_deref() {
        hint.with_extension(extension);
    }
    if let Some(mime) = media.mime_type.as_deref() {
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

    if media.seek_in_format {
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

    let mut discard_samples = media
        .discard_ms
        .saturating_mul(output_rate as u64)
        .saturating_mul(output_channels as u64)
        / 1000;
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
        let mut samples = convert_audio(
            &samples,
            input_channels,
            input_rate,
            output_channels,
            output_rate,
        );
        if discard_samples > 0 {
            let discard = (discard_samples as usize).min(samples.len());
            samples.drain(..discard);
            discard_samples -= discard as u64;
            if samples.is_empty() {
                continue;
            }
        }
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
