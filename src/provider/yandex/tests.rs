use serde_json::json;
use url::Url;
use yandex_music::model::track::Track;

use crate::model::{PlaybackCapability, ProviderKind};

use super::{YandexPlaylistRef, normalizovat_track, parse_playlist_url};

#[test]
fn numeric_playlist_url_is_parsed() {
    let parsed = parse_playlist_url(
        &Url::parse("https://music.yandex.ru/users/12345/playlists/77?utm_source=share").unwrap(),
    );
    assert_eq!(
        parsed,
        Some(YandexPlaylistRef::Numeric {
            user_id: 12_345,
            kind: 77,
        })
    );
}

#[test]
fn named_and_uuid_playlist_urls_are_parsed() {
    assert_eq!(
        parse_playlist_url(&Url::parse("https://music.yandex.ru/users/vasya/playlists/3").unwrap()),
        Some(YandexPlaylistRef::Named {
            login: "vasya".to_string(),
            kind: 3,
        })
    );
    assert_eq!(
        parse_playlist_url(&Url::parse("https://music.yandex.ru/playlists/dead-beef").unwrap()),
        Some(YandexPlaylistRef::Uuid("dead-beef".to_string()))
    );
}

#[test]
fn foreign_host_cannot_sneak_into_import() {
    assert_eq!(
        parse_playlist_url(
            &Url::parse("https://music.yandex.ru.evil.example/users/1/playlists/2").unwrap()
        ),
        None
    );
}

#[test]
fn track_metadata_is_normalized() {
    let raw: Track = serde_json::from_value(json!({
        "id": 42,
        "realId": "42",
        "title": "Трек",
        "available": false,
        "artists": [{"id": 7, "name": "Артист", "genres": ["electronic"]}],
        "albums": [],
        "durationMs": 123000,
        "coverUri": "avatars.yandex.net/get-music-content/%%",
        "explicit": true
    }))
    .unwrap();
    let track = normalizovat_track(raw).unwrap();
    assert_eq!(track.provider, ProviderKind::YandexMusic);
    assert_eq!(track.id, "42");
    assert_eq!(track.artists, vec!["Артист"]);
    assert_eq!(track.duration_ms, Some(123_000));
    assert_eq!(track.genres, vec!["electronic"]);
    assert!(matches!(
        track.capability,
        PlaybackCapability::Unavailable { .. }
    ));
}
