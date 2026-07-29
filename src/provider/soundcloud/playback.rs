use std::collections::BTreeMap;

use anyhow::{Context, Result, ensure};
use url::Url;

use crate::model::{PlaybackCapability, PlaybackSource, ProviderKind, TrackRef};

use super::{client::SoundCloudClient, models::ScStreams};

pub(super) async fn poluchit_istochnik(
    client: &SoundCloudClient,
    track: &TrackRef,
) -> Result<PlaybackSource> {
    ensure!(
        track.provider == ProviderKind::SoundCloud,
        "для SoundCloud нужен трек SoundCloud"
    );
    ensure!(track.capability.can_play(), "трек SoundCloud недоступен");

    let urn = if track.id.starts_with("soundcloud:tracks:") {
        track.id.clone()
    } else {
        format!("soundcloud:tracks:{}", track.id)
    };
    let streams: ScStreams = client
        .get_json(client.public_url(&["tracks", &urn, "streams"])?, &[])
        .await
        .context("SoundCloud не выдал поток трека")?;
    let (raw_url, mime_type, supports_range) =
        select_stream(&streams, &track.capability).context("у трека нет доступного потока")?;
    let url = Url::parse(raw_url).context("SoundCloud вернул неверный адрес потока")?;
    ensure!(
        url.scheme() == "https" && url.host_str().is_some(),
        "SoundCloud вернул небезопасный адрес потока"
    );

    Ok(PlaybackSource {
        url,
        headers: BTreeMap::new(),
        mime_type: Some(mime_type.to_string()),
        supports_range,
        expires_at_ms: None,
        capability: track.capability.clone(),
    })
}

fn select_stream<'a>(
    streams: &'a ScStreams,
    capability: &PlaybackCapability,
) -> Option<(&'a str, &'static str, bool)> {
    match capability {
        PlaybackCapability::Preview { .. } => streams
            .preview_mp3_128_url
            .as_deref()
            .map(|url| (url, "audio/mpeg", true)),
        PlaybackCapability::Full => streams
            .hls_aac_160_url
            .as_deref()
            .or(streams.hls_aac_96_url.as_deref())
            .map(|url| (url, "application/vnd.apple.mpegurl", false))
            .or_else(|| {
                streams
                    .http_mp3_128_url
                    .as_deref()
                    .map(|url| (url, "audio/mpeg", true))
            })
            .or_else(|| {
                streams
                    .hls_mp3_128_url
                    .as_deref()
                    .or(streams.hls_opus_64_url.as_deref())
                    .map(|url| (url, "application/vnd.apple.mpegurl", false))
            }),
        PlaybackCapability::Unavailable { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    use super::*;

    #[tokio::test]
    async fn modern_aac_hls_is_preferred() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0u8; 4096];
            let read = socket.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            assert!(request.contains("GET /tracks/soundcloud:tracks:42/streams?"));
            let body = r#"{"hls_aac_160_url":"https://cf-hls-media.sndcdn.com/track.m3u8","http_mp3_128_url":"https://cf-media.sndcdn.com/legacy.mp3"}"#;
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let client = SoundCloudClient::with_base(
            "fake-key".to_string(),
            Url::parse(&format!("http://{address}/")).unwrap(),
        )
        .unwrap();
        let source = poluchit_istochnik(&client, &track(PlaybackCapability::Full))
            .await
            .unwrap();
        server.join().unwrap();
        assert_eq!(
            source.url.as_str(),
            "https://cf-hls-media.sndcdn.com/track.m3u8"
        );
        assert_eq!(
            source.mime_type.as_deref(),
            Some("application/vnd.apple.mpegurl")
        );
        assert!(!source.supports_range);
    }

    #[test]
    fn preview_does_not_steal_the_full_stream() {
        let streams = ScStreams {
            hls_aac_160_url: Some("https://cdn.example/full.m3u8".to_string()),
            preview_mp3_128_url: Some("https://cdn.example/preview.mp3".to_string()),
            ..ScStreams::default()
        };
        assert_eq!(
            select_stream(&streams, &PlaybackCapability::Preview { seconds: 30 }).unwrap(),
            ("https://cdn.example/preview.mp3", "audio/mpeg", true)
        );
    }

    fn track(capability: PlaybackCapability) -> TrackRef {
        TrackRef {
            provider: ProviderKind::SoundCloud,
            id: "42".to_string(),
            title: "Трек".to_string(),
            artists: vec!["Автор".to_string()],
            duration_ms: None,
            artwork_url: None,
            web_url: Url::parse("https://soundcloud.com/test/track").unwrap(),
            capability,
            genres: Vec::new(),
            explicit: false,
        }
    }
}
