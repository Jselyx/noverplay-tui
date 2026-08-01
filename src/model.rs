use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchProvider {
    #[default]
    All,
    SoundCloud,
    YandexMusic,
    Deezer,
}

impl SearchProvider {
    pub const fn next(self) -> Self {
        match self {
            Self::All => Self::SoundCloud,
            Self::SoundCloud => Self::YandexMusic,
            Self::YandexMusic => Self::Deezer,
            Self::Deezer => Self::All,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "Все",
            Self::SoundCloud => "SoundCloud",
            Self::YandexMusic => "Yandex",
            Self::Deezer => "Deezer",
        }
    }

    pub const fn provider(self) -> Option<ProviderKind> {
        match self {
            Self::All => None,
            Self::SoundCloud => Some(ProviderKind::SoundCloud),
            Self::YandexMusic => Some(ProviderKind::YandexMusic),
            Self::Deezer => Some(ProviderKind::Deezer),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    #[default]
    SoundCloud,
    YandexMusic,
    Deezer,
}

impl ProviderKind {
    pub fn from_url(value: &str) -> Option<Self> {
        let host = Url::parse(value).ok()?.host_str()?.to_ascii_lowercase();
        if host == "soundcloud.com" || host.ends_with(".soundcloud.com") {
            Some(Self::SoundCloud)
        } else if host == "music.yandex.ru" || host.ends_with(".music.yandex.ru") {
            Some(Self::YandexMusic)
        } else if host == "deezer.com" || host.ends_with(".deezer.com") {
            Some(Self::Deezer)
        } else {
            None
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::SoundCloud => "SoundCloud",
            Self::YandexMusic => "Yandex Music",
            Self::Deezer => "Deezer",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlaybackCapability {
    Full,
    Preview { seconds: u16 },
    Unavailable { reason: String },
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

impl PlaybackCapability {
    pub const fn can_play(&self) -> bool {
        !matches!(self, Self::Unavailable { .. })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TrackRef {
    pub provider: ProviderKind,
    pub id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub duration_ms: Option<u64>,
    pub artwork_url: Option<Url>,
    pub web_url: Url,
    pub capability: PlaybackCapability,
    pub genres: Vec<String>,
    pub explicit: bool,
    #[serde(default)]
    pub drm: bool,
}

impl TrackRef {
    pub fn provider_key(&self) -> String {
        format!("{}:{}", self.provider.label(), self.id.trim())
    }

    pub fn canonical_key(&self) -> String {
        let artists = self
            .artists
            .iter()
            .map(|artist| normalizovat_text(artist))
            .collect::<Vec<_>>()
            .join("|");
        format!("{}::{artists}", normalizovat_text(&self.title))
    }

    pub const fn protection_badge(&self) -> Option<&'static str> {
        if self.drm { Some("DRM") } else { None }
    }

    pub fn display_artist(&self) -> String {
        if self.artists.is_empty() {
            "Неизвестный исполнитель".to_string()
        } else {
            self.artists.join(", ")
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Playlist {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub source_url: Option<Url>,
    pub tracks: Vec<TrackRef>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

impl Playlist {
    pub fn new(title: impl Into<String>, now_ms: i64) -> Self {
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            description: String::new(),
            source_url: None,
            tracks: Vec::new(),
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
        }
    }

    pub fn push_unique(&mut self, track: TrackRef) -> bool {
        let provider_key = track.provider_key();
        let canonical_key = track.canonical_key();
        if self.tracks.iter().any(|current| {
            current.provider_key() == provider_key || current.canonical_key() == canonical_key
        }) {
            return false;
        }
        self.tracks.push(track);
        true
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PlaybackSource {
    pub url: Url,
    pub headers: BTreeMap<String, String>,
    pub mime_type: Option<String>,
    pub supports_range: bool,
    pub expires_at_ms: Option<i64>,
    pub capability: PlaybackCapability,
}

fn normalizovat_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(provider: ProviderKind, id: &str, title: &str, artist: &str) -> TrackRef {
        TrackRef {
            provider,
            id: id.to_string(),
            title: title.to_string(),
            artists: vec![artist.to_string()],
            duration_ms: Some(180_000),
            artwork_url: None,
            web_url: Url::parse("https://example.com/track").unwrap(),
            capability: PlaybackCapability::Full,
            genres: Vec::new(),
            explicit: false,
            drm: false,
        }
    }

    #[test]
    fn provider_is_detected_from_playlist_url() {
        assert_eq!(
            ProviderKind::from_url("https://soundcloud.com/user/sets/mix"),
            Some(ProviderKind::SoundCloud)
        );
        assert_eq!(
            ProviderKind::from_url("https://music.yandex.ru/users/me/playlists/1"),
            Some(ProviderKind::YandexMusic)
        );
        assert_eq!(
            ProviderKind::from_url("https://www.deezer.com/playlist/1"),
            Some(ProviderKind::Deezer)
        );
    }

    #[test]
    fn playlist_deduplicates_tracks_across_providers() {
        let mut playlist = Playlist::new("Тест", 1);
        assert!(playlist.push_unique(track(
            ProviderKind::SoundCloud,
            "1",
            "  Song Name ",
            "Artist"
        )));
        assert!(!playlist.push_unique(track(ProviderKind::Deezer, "2", "song   name", "artist")));
    }

    #[test]
    fn search_provider_cycles_all_then_each_platform() {
        assert_eq!(SearchProvider::All.next(), SearchProvider::SoundCloud);
        assert_eq!(
            SearchProvider::SoundCloud.next(),
            SearchProvider::YandexMusic
        );
        assert_eq!(SearchProvider::YandexMusic.next(), SearchProvider::Deezer);
        assert_eq!(SearchProvider::Deezer.next(), SearchProvider::All);
    }

    #[test]
    fn protected_tracks_have_an_explicit_drm_marker() {
        let mut value = track(ProviderKind::SoundCloud, "1", "Track", "Artist");
        value.drm = true;
        assert_eq!(value.protection_badge(), Some("DRM"));
    }
}
