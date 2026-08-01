use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use reqwest::{Client, header::COOKIE};
use serde::Deserialize;
use url::Url;

use crate::{
    model::{PlaybackCapability, PlaybackSource, ProviderKind, TrackRef},
    provider::{Attribution, ImportedPlaylist, MusicProvider, SearchPage},
};

const API: &str = "https://api.deezer.com";
const PAGE_SIZE: usize = 50;

pub struct DeezerProvider {
    http: Client,
    arl: String,
}

impl DeezerProvider {
    pub fn new(value: impl AsRef<str>) -> Result<Self> {
        let arl = normalize_arl(value.as_ref());
        if arl.is_empty() {
            bail!("для Deezer нужен cookie arl")
        }
        let http = Client::builder()
            .user_agent(format!("noverplay-tui/{}", crate::APP_VERSION))
            .build()
            .context("не удалось создать HTTP-клиент Deezer")?;
        Ok(Self { http, arl })
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, url: Url) -> Result<T> {
        let response = self
            .http
            .get(url)
            .header(COOKIE, format!("arl={}", self.arl))
            .send()
            .await
            .context("Deezer не ответил")?
            .error_for_status()
            .context("Deezer отклонил запрос")?;
        response
            .json()
            .await
            .context("Deezer вернул непонятный JSON")
    }

    async fn playlist_page(&self, url: Url) -> Result<ApiPage<ApiTrack>> {
        self.get(url).await
    }
}

#[async_trait]
impl MusicProvider for DeezerProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Deezer
    }

    fn attribution(&self) -> Attribution {
        Attribution {
            label: "Deezer".to_string(),
            url: Url::parse("https://www.deezer.com").expect("статический адрес Deezer"),
        }
    }

    async fn search(&self, query: &str, cursor: Option<&str>) -> Result<SearchPage> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(SearchPage::default());
        }
        let index = cursor
            .and_then(|value| value.parse().ok())
            .unwrap_or(0usize);
        let mut url = Url::parse(&format!("{API}/search/track"))?;
        url.query_pairs_mut()
            .append_pair("q", query)
            .append_pair("index", &index.to_string())
            .append_pair("limit", &PAGE_SIZE.to_string());
        let page: ApiPage<ApiTrack> = self.get(url).await?;
        let next_cursor = page.next.as_ref().and_then(|url| {
            Url::parse(url)
                .ok()?
                .query_pairs()
                .find_map(|(key, value)| (key == "index").then(|| value.into_owned()))
        });
        Ok(SearchPage {
            tracks: page.data.into_iter().filter_map(map_track).collect(),
            next_cursor,
        })
    }

    async fn import_playlist(&self, source: &Url) -> Result<ImportedPlaylist> {
        let id =
            entity_id(source, "playlist").context("не удалось определить ID плейлиста Deezer")?;
        let details: ApiPlaylist = self
            .get(Url::parse(&format!("{API}/playlist/{id}"))?)
            .await?;
        let mut tracks = details.tracks.data;
        let mut next = details.tracks.next;
        while let Some(url) = next.take() {
            let page = self.playlist_page(Url::parse(&url)?).await?;
            tracks.extend(page.data);
            next = page.next;
        }
        Ok(ImportedPlaylist {
            title: non_empty(details.title, "Deezer playlist"),
            description: details.description.unwrap_or_default(),
            source_url: source.clone(),
            tracks: tracks.into_iter().filter_map(map_track).collect(),
        })
    }

    async fn related(&self, track: &TrackRef, limit: usize) -> Result<Vec<TrackRef>> {
        let url = Url::parse(&format!(
            "{API}/track/{}/radio?limit={}",
            track.id,
            limit.clamp(1, 100)
        ))?;
        let page: ApiPage<ApiTrack> = self.get(url).await?;
        Ok(page.data.into_iter().filter_map(map_track).collect())
    }

    async fn playback_source(&self, track: &TrackRef) -> Result<PlaybackSource> {
        if !track.capability.can_play() {
            bail!("Deezer-трек помечен как недоступный")
        }
        let details: ApiTrack = self
            .get(Url::parse(&format!("{API}/track/{}", track.id))?)
            .await?;
        let preview = details
            .preview
            .filter(|value| !value.trim().is_empty())
            .context("Deezer не дал официальный preview; полный поток защищён DRM")?;
        Ok(PlaybackSource {
            url: Url::parse(&preview).context("Deezer вернул повреждённый preview URL")?,
            headers: BTreeMap::new(),
            mime_type: Some("audio/mpeg".to_string()),
            supports_range: true,
            expires_at_ms: None,
            capability: PlaybackCapability::Preview { seconds: 30 },
        })
    }
}

fn normalize_arl(value: &str) -> String {
    let value = value
        .trim()
        .strip_prefix("Cookie:")
        .unwrap_or(value.trim())
        .trim();
    value
        .split(';')
        .find_map(|part| {
            let part = part.trim();
            if let Some((name, value)) = part.split_once('=')
                && name.trim().eq_ignore_ascii_case("arl")
            {
                return Some(value.trim().trim_matches(['\'', '"']).to_string());
            }
            None
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| value.trim_matches(['\'', '"']).to_string())
}

fn entity_id(url: &Url, kind: &str) -> Option<String> {
    let segments = url.path_segments()?.collect::<Vec<_>>();
    segments.windows(2).find_map(|pair| {
        (pair[0].eq_ignore_ascii_case(kind) && pair[1].chars().all(|ch| ch.is_ascii_digit()))
            .then(|| pair[1].to_string())
    })
}

fn map_track(track: ApiTrack) -> Option<TrackRef> {
    let id = track.id?.to_string();
    let preview_available = track
        .preview
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty());
    Some(TrackRef {
        provider: ProviderKind::Deezer,
        id: id.clone(),
        title: non_empty(track.title, "Без названия"),
        artists: track
            .artist
            .and_then(|artist| artist.name)
            .into_iter()
            .collect(),
        duration_ms: track.duration.map(|seconds| seconds.saturating_mul(1000)),
        artwork_url: track
            .album
            .and_then(|album| album.cover_xl.or(album.cover_big))
            .and_then(|url| Url::parse(&url).ok()),
        web_url: track
            .link
            .and_then(|url| Url::parse(&url).ok())
            .unwrap_or_else(|| Url::parse(&format!("https://www.deezer.com/track/{id}")).unwrap()),
        capability: if preview_available {
            PlaybackCapability::Preview { seconds: 30 }
        } else {
            PlaybackCapability::Unavailable {
                reason: "полный поток Deezer защищён DRM".to_string(),
            }
        },
        genres: Vec::new(),
        explicit: track.explicit_lyrics.unwrap_or(false),
        drm: true,
    })
}

fn non_empty(value: Option<String>, fallback: &str) -> String {
    value
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

#[derive(Deserialize)]
struct ApiPage<T> {
    #[serde(default)]
    data: Vec<T>,
    next: Option<String>,
}

#[derive(Deserialize)]
struct ApiPlaylist {
    title: Option<String>,
    description: Option<String>,
    tracks: ApiPage<ApiTrack>,
}

#[derive(Default, Deserialize)]
struct ApiTrack {
    id: Option<u64>,
    title: Option<String>,
    duration: Option<u64>,
    link: Option<String>,
    preview: Option<String>,
    explicit_lyrics: Option<bool>,
    artist: Option<ApiArtist>,
    album: Option<ApiAlbum>,
}

#[derive(Deserialize)]
struct ApiArtist {
    name: Option<String>,
}

#[derive(Deserialize)]
struct ApiAlbum {
    cover_xl: Option<String>,
    cover_big: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arl_accepts_raw_value_and_cookie_header() {
        assert_eq!(normalize_arl("token"), "token");
        assert_eq!(normalize_arl("Cookie: foo=1; arl='token'; path=/"), "token");
    }

    #[test]
    fn playlist_id_survives_locale_prefix() {
        let url = Url::parse("https://www.deezer.com/ru/playlist/12345").unwrap();
        assert_eq!(entity_id(&url, "playlist").as_deref(), Some("12345"));
    }
}
