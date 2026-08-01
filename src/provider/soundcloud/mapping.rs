use url::Url;

use crate::model::{PlaybackCapability, ProviderKind, TrackRef};

use super::models::ScTrack;

pub fn normalizovat_track(track: ScTrack) -> Option<TrackRef> {
    let id = track.id.trim().to_string();
    if id.is_empty() {
        return None;
    }
    let capability = match track.access.as_deref() {
        Some("blocked") => PlaybackCapability::Unavailable {
            reason: "SoundCloud запретил внешнее воспроизведение".to_string(),
        },
        Some("preview") => PlaybackCapability::Preview { seconds: 30 },
        _ if track.streamable == Some(false)
            || track
                .policy
                .as_deref()
                .is_some_and(|value| value == "BLOCK") =>
        {
            PlaybackCapability::Unavailable {
                reason: "автор или регион запретил воспроизведение".to_string(),
            }
        }
        _ => PlaybackCapability::Full,
    };
    let artwork_url = track
        .artwork_url
        .as_deref()
        .map(|value| value.replace("-large.", "-t500x500."))
        .and_then(|value| Url::parse(&value).ok());
    let web_url = track
        .permalink_url
        .as_deref()
        .and_then(|value| Url::parse(value).ok())
        .or_else(|| Url::parse(&format!("https://soundcloud.com/tracks/{id}")).ok())?;
    let genres = track
        .genre
        .into_iter()
        .map(|genre| genre.trim().to_string())
        .filter(|genre| !genre.is_empty())
        .collect();

    Some(TrackRef {
        provider: ProviderKind::SoundCloud,
        id,
        title: if track.title.trim().is_empty() {
            "Без названия".to_string()
        } else {
            track.title
        },
        artists: (!track.user.username.trim().is_empty())
            .then_some(track.user.username)
            .into_iter()
            .collect(),
        duration_ms: track.duration,
        artwork_url,
        web_url,
        capability,
        genres,
        explicit: false,
        drm: false,
    })
}
