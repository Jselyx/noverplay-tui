use url::Url;
use yandex_music::model::track::Track;

use crate::model::{PlaybackCapability, ProviderKind, TrackRef};

pub fn normalizovat_track(track: Track) -> Option<TrackRef> {
    let id = track.id.trim().to_string();
    if id.is_empty() {
        return None;
    }
    let artists = track
        .artists
        .iter()
        .filter_map(|artist| artist.name.as_deref())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    let genres = track
        .artists
        .iter()
        .flat_map(|artist| artist.genres.iter().flatten())
        .map(|genre| genre.trim())
        .filter(|genre| !genre.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    let artwork_url = track
        .cover_uri
        .as_deref()
        .or(track.og_image.as_deref())
        .and_then(cover_url);
    let capability = if track.available == Some(false) {
        PlaybackCapability::Unavailable {
            reason: "трек недоступен этому аккаунту Yandex Music".to_string(),
        }
    } else {
        PlaybackCapability::Full
    };

    Some(TrackRef {
        provider: ProviderKind::YandexMusic,
        id: id.clone(),
        title: track
            .title
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| "Без названия".to_string()),
        artists,
        duration_ms: track.duration.map(|duration| duration.as_millis() as u64),
        artwork_url,
        web_url: Url::parse(&format!("https://music.yandex.ru/track/{id}")).ok()?,
        capability,
        genres,
        explicit: track.explicit.unwrap_or(false),
        drm: false,
    })
}

fn cover_url(raw: &str) -> Option<Url> {
    let value = raw.trim().replace("%%", "1000x1000");
    if value.is_empty() {
        return None;
    }
    let normalized = if value.starts_with("https://") || value.starts_with("http://") {
        value
    } else if value.starts_with("//") {
        format!("https:{value}")
    } else {
        format!("https://{}", value.trim_start_matches('/'))
    };
    Url::parse(&normalized).ok()
}
