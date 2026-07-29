use std::collections::BTreeMap;

use anyhow::{Context, Result, ensure};
use url::Url;
use yandex_music::{
    YandexMusicClient,
    api::track::get_file_info::{Codec, GetFileInfoOptions},
    model::info::file_info::{Quality, TrackFileInfo},
};

use crate::model::{PlaybackCapability, PlaybackSource, ProviderKind, TrackRef};

pub(super) async fn poluchit_istochnik(
    client: &YandexMusicClient,
    track: &TrackRef,
) -> Result<PlaybackSource> {
    ensure!(
        track.provider == ProviderKind::YandexMusic,
        "трек принадлежит другому провайдеру"
    );
    ensure!(
        track.capability.can_play(),
        "трек недоступен этому аккаунту"
    );
    let info = client
        .get_file_info(
            &GetFileInfoOptions::new(&track.id)
                .quality(Quality::Normal)
                .codecs([Codec::Mp3, Codec::Aac, Codec::AacMp4])
                .is_encrypted(false),
        )
        .await
        .context("Yandex Music не выдал незашифрованный источник")?;
    let url = vybrat_url_potoka(&info).context("Yandex Music не вернул пригодную аудиоссылку")?;

    // Просили без drm, поэтому encraw даже не нюхаем, пусть этот чемодан несет официальный клиент))))
    Ok(PlaybackSource {
        url,
        headers: BTreeMap::new(),
        mime_type: codec_mime_type(&info.codec).map(str::to_string),
        supports_range: true,
        expires_at_ms: None,
        capability: PlaybackCapability::Full,
    })
}

fn vybrat_url_potoka(info: &TrackFileInfo) -> Option<Url> {
    info.urls
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(info.url.as_str()))
        .filter(|value| {
            let normalized = value.to_ascii_lowercase();
            !normalized.contains("preview") && !normalized.contains("snippet")
        })
        .find_map(|value| {
            let url = Url::parse(value).ok()?;
            (url.scheme() == "https" && url.host_str().is_some()).then_some(url)
        })
}

fn codec_mime_type(codec: &str) -> Option<&'static str> {
    match codec.to_ascii_lowercase().as_str() {
        "mp3" => Some("audio/mpeg"),
        "aac" | "he-aac" => Some("audio/aac"),
        "aac-mp4" | "he-aac-mp4" => Some("audio/mp4"),
        "flac" => Some("audio/flac"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_info(urls: Vec<String>, fallback: &str) -> TrackFileInfo {
        TrackFileInfo {
            bitrate: 320,
            codec: "mp3".to_string(),
            gain: false,
            quality: "nq".to_string(),
            real_id: "1".to_string(),
            size: 123,
            track_id: "1".to_string(),
            transport: "raw".to_string(),
            url: fallback.to_string(),
            urls,
        }
    }

    #[test]
    fn secure_full_stream_is_preferred() {
        let info = file_info(
            vec![
                "http://cdn.example/track.mp3".to_string(),
                "https://cdn.example/track.mp3".to_string(),
            ],
            "https://cdn.example/fallback.mp3",
        );
        assert_eq!(
            vybrat_url_potoka(&info).unwrap().as_str(),
            "https://cdn.example/track.mp3"
        );
    }

    #[test]
    fn preview_is_rejected_even_when_it_looks_convenient() {
        let info = file_info(
            vec!["https://cdn.example/preview.mp3".to_string()],
            "https://cdn.example/full.mp3",
        );
        assert_eq!(
            vybrat_url_potoka(&info).unwrap().as_str(),
            "https://cdn.example/full.mp3"
        );
    }
}
