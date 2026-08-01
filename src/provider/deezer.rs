use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use blowfish::Blowfish;
use cbc::{
    Decryptor,
    cipher::{BlockDecryptMut, KeyIvInit, block_padding::NoPadding},
};
use reqwest::{
    Client,
    header::{COOKIE, USER_AGENT},
};
use serde::Deserialize;
use serde_json::{Value, json};
use url::Url;

use crate::{
    model::{PlaybackCapability, PlaybackSource, ProviderKind, TrackRef},
    provider::{Attribution, ImportedPlaylist, MusicProvider, SearchPage},
};

const API: &str = "https://api.deezer.com";
const PAGE_SIZE: usize = 50;
const GATEWAY: &str = "https://www.deezer.com/ajax/gw-light.php";
const MEDIA: &str = "https://media.deezer.com/v1/get_url";
const BF_SECRET: &[u8; 16] = b"g4el58wc0zvf9na1";
const BF_IV: [u8; 8] = [0, 1, 2, 3, 4, 5, 6, 7];
const ENCRYPTED_CHUNK: usize = 2048;

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

    async fn gateway(&self, method: &str, token: &str, params: Value) -> Result<Value> {
        let mut url = Url::parse(GATEWAY)?;
        url.query_pairs_mut()
            .append_pair("api_version", "1.0")
            .append_pair("api_token", if token.is_empty() { "null" } else { token })
            .append_pair("input", "3")
            .append_pair("method", method);
        let payload = self
            .http
            .post(url)
            .header(COOKIE, format!("arl={}", self.arl))
            .json(&params)
            .send()
            .await
            .context("Deezer gateway не ответил")?
            .error_for_status()
            .context("Deezer gateway отклонил запрос")?
            .json::<Value>()
            .await
            .context("Deezer gateway вернул непонятный JSON")?;
        if payload.get("error").is_some_and(gateway_has_error) {
            bail!("Deezer gateway: {}", payload["error"])
        }
        Ok(payload)
    }

    async fn prepare_full_track(&self, track_id: &str) -> Result<PathBuf> {
        let cached_mp3 = deezer_cache_dir().join(format!("{track_id}.mp3"));
        let cached_flac = deezer_cache_dir().join(format!("{track_id}.flac"));
        if cached_mp3
            .metadata()
            .is_ok_and(|metadata| metadata.len() > 0)
        {
            return Ok(cached_mp3);
        }
        if cached_flac
            .metadata()
            .is_ok_and(|metadata| metadata.len() > 0)
        {
            return Ok(cached_flac);
        }
        let user = self.gateway("deezer.getUserData", "", json!({})).await?;
        let token = text_at(&user, &["results", "checkForm"])
            .context("ARL не авторизован: Deezer не выдал checkForm")?;
        let license = text_at(&user, &["results", "USER", "OPTIONS", "license_token"])
            .context("ARL не авторизован: Deezer не выдал license_token")?;
        let raw = self
            .gateway("song.getListData", token, json!({ "SNG_IDS": [track_id] }))
            .await?;
        let song = raw
            .pointer("/results/data/0")
            .context("Deezer не вернул метаданные полного трека")?;
        let track_token =
            text_at(song, &["TRACK_TOKEN"]).context("Deezer не вернул TRACK_TOKEN")?;
        let (media_url, extension) = self.resolve_media(license, track_token).await?;
        let bytes = self
            .http
            .get(media_url)
            .header(USER_AGENT, format!("noverplay-tui/{}", crate::APP_VERSION))
            .send()
            .await
            .context("не удалось скачать полный трек Deezer")?
            .error_for_status()
            .context("Deezer CDN отклонил полный трек")?
            .bytes()
            .await
            .context("не удалось прочитать полный трек Deezer")?;
        let decrypted = decrypt_audio(&bytes, track_id)?;
        let path = deezer_cache_dir().join(format!("{track_id}.{extension}"));
        write_cache(&path, &decrypted)?;
        Ok(path)
    }

    async fn resolve_media(
        &self,
        license: &str,
        track_token: &str,
    ) -> Result<(String, &'static str)> {
        for (format, extension) in [("MP3_320", "mp3"), ("MP3_128", "mp3"), ("FLAC", "flac")] {
            let payload = self
                .http
                .post(MEDIA)
                .json(&json!({
                    "license_token": license,
                    "media": [{ "type": "FULL", "formats": [{
                        "cipher": "BF_CBC_STRIPE", "format": format
                    }]}],
                    "track_tokens": [track_token]
                }))
                .send()
                .await
                .context("Deezer media API не ответил")?;
            if !payload.status().is_success() {
                continue;
            }
            let value = payload
                .json::<Value>()
                .await
                .context("повреждённый ответ Deezer media API")?;
            if let Some(url) = value
                .pointer("/data/0/media/0/sources/0/url")
                .and_then(Value::as_str)
                && !url.trim().is_empty()
            {
                return Ok((url.to_string(), extension));
            }
        }
        bail!("Deezer не выдал полный поток для этого ARL/трека")
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
        let path = self.prepare_full_track(&track.id).await?;
        full_cache_source(&path)
    }
}

fn gateway_has_error(value: &Value) -> bool {
    match value {
        Value::Array(items) => !items.is_empty(),
        Value::Object(items) => !items.is_empty(),
        _ => false,
    }
}

fn text_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    path.iter()
        .try_fold(value, |current, key| current.get(*key))?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn decrypt_audio(encrypted: &[u8], track_id: &str) -> Result<Vec<u8>> {
    let key = blowfish_key(track_id);
    let mut output = Vec::with_capacity(encrypted.len());
    for (index, chunk) in encrypted.chunks(ENCRYPTED_CHUNK).enumerate() {
        if chunk.len() == ENCRYPTED_CHUNK && index % 3 == 0 {
            let mut buffer = chunk.to_vec();
            let decryptor = Decryptor::<Blowfish>::new_from_slices(&key, &BF_IV)
                .map_err(|error| anyhow::anyhow!("Deezer Blowfish init: {error}"))?;
            let decoded = decryptor
                .decrypt_padded_mut::<NoPadding>(&mut buffer)
                .map_err(|error| anyhow::anyhow!("Deezer Blowfish decrypt: {error}"))?;
            output.extend_from_slice(decoded);
        } else {
            output.extend_from_slice(chunk);
        }
    }
    Ok(output)
}

fn blowfish_key(track_id: &str) -> [u8; 16] {
    let digest = format!("{:x}", md5::compute(track_id.as_bytes()));
    let bytes = digest.as_bytes();
    let mut key = [0; 16];
    for index in 0..16 {
        key[index] = bytes[index] ^ bytes[index + 16] ^ BF_SECRET[index];
    }
    key
}

fn deezer_cache_dir() -> PathBuf {
    std::env::temp_dir().join("noverplay").join("deezer-cache")
}

fn write_cache(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("повреждённый путь кэша Deezer")?;
    fs::create_dir_all(parent).context("не удалось создать кэш Deezer")?;
    let temporary = path.with_extension("part");
    fs::write(&temporary, bytes).context("не удалось записать кэш Deezer")?;
    if path.exists() {
        fs::remove_file(path).context("не удалось обновить кэш Deezer")?;
    }
    fs::rename(temporary, path).context("не удалось завершить кэш Deezer")
}

fn full_cache_source(path: &Path) -> Result<PlaybackSource> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("mp3");
    Ok(PlaybackSource {
        url: Url::from_file_path(path)
            .map_err(|_| anyhow::anyhow!("повреждённый путь кэша Deezer"))?,
        headers: BTreeMap::new(),
        mime_type: Some(
            if extension.eq_ignore_ascii_case("flac") {
                "audio/flac"
            } else {
                "audio/mpeg"
            }
            .to_string(),
        ),
        supports_range: true,
        expires_at_ms: None,
        capability: PlaybackCapability::Full,
    })
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
        capability: PlaybackCapability::Full,
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
    fn full_cache_source_is_not_downgraded_to_preview() {
        let path = std::path::Path::new("C:/cache/deezer-track.mp3");
        let source = full_cache_source(path).unwrap();
        assert_eq!(source.capability, PlaybackCapability::Full);
        assert_eq!(source.mime_type.as_deref(), Some("audio/mpeg"));
        assert_eq!(source.url.scheme(), "file");
    }

    #[test]
    fn playlist_id_survives_locale_prefix() {
        let url = Url::parse("https://www.deezer.com/ru/playlist/12345").unwrap();
        assert_eq!(entity_id(&url, "playlist").as_deref(), Some("12345"));
    }
}
