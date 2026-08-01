use anyhow::{Result, ensure};

use crate::{
    model::{ProviderKind, TrackRef},
    provider::soundcloud::{client::SoundCloudClient, models::ScCollection, normalizovat_track},
};

const MAX_RELATED_TRACKS: usize = 200;

pub(super) async fn related_tracks(
    client: &SoundCloudClient,
    track: &TrackRef,
    limit: usize,
) -> Result<Vec<TrackRef>> {
    ensure!(
        track.provider == ProviderKind::SoundCloud,
        "для рекомендаций SoundCloud нужен трек SoundCloud"
    );
    if limit == 0 {
        return Ok(Vec::new());
    }

    let limit = limit.min(MAX_RELATED_TRACKS);
    let response: ScCollection<_> = client
        .get_json(
            client.v2_url(&["tracks", &track.id, "related"])?,
            &[
                ("limit", limit.to_string()),
                ("linked_partitioning", "true".to_string()),
                ("access", "playable,preview".to_string()),
            ],
        )
        .await?;

    // Related любит вернуть исходный трек первым, очень свежо и вообще никто бы не догадался
    Ok(response
        .collection
        .into_iter()
        .filter_map(normalizovat_track)
        .filter(|candidate| candidate.id != track.id)
        .take(limit)
        .collect())
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    use url::Url;

    use crate::model::PlaybackCapability;

    use super::*;

    #[tokio::test]
    async fn related_skips_the_seed_track() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0u8; 4096];
            let read = socket.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            assert!(request.contains("GET /tracks/42/related?"));
            assert!(request.contains("limit=5"));
            let body = r#"{"collection":[{"id":42,"title":"Исходный","permalink_url":"https://soundcloud.com/test/seed","user":{"username":"Автор"}},{"id":9,"title":"Соседний","permalink_url":"https://soundcloud.com/test/next","user":{"username":"Другой"}}]}"#;
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
        let track = TrackRef {
            provider: ProviderKind::SoundCloud,
            id: "42".to_string(),
            title: "Исходный".to_string(),
            artists: vec!["Автор".to_string()],
            duration_ms: None,
            artwork_url: None,
            web_url: Url::parse("https://soundcloud.com/test/seed").unwrap(),
            capability: PlaybackCapability::Full,
            genres: Vec::new(),
            explicit: false,
            drm: false,
        };
        let tracks = related_tracks(&client, &track, 5).await.unwrap();
        server.join().unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].id, "9");
    }
}
