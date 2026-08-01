use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use aes::{
    Aes128,
    cipher::{BlockEncrypt, KeyInit as AesKeyInit, generic_array::GenericArray},
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
    cookie::Jar,
    header::{ACCEPT, ACCEPT_LANGUAGE, CONTENT_TYPE, HeaderMap, HeaderValue, USER_AGENT},
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
const COOKIE_URL: &str = "https://www.deezer.com/";
const COOKIE_URL_HTTP: &str = "http://www.deezer.com/";
const BROWSER_UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/96.0.4664.110 Safari/537.36";
const LEGACY_AES_KEY: &[u8; 16] = b"jo6aey6haid2Teih";
const BF_SECRET: &[u8; 16] = b"g4el58wc0zvf9na1";
const BF_IV: [u8; 8] = [0, 1, 2, 3, 4, 5, 6, 7];
const ENCRYPTED_CHUNK: usize = 2048;

pub struct DeezerProvider {
    http: Client,
}

impl DeezerProvider {
    pub fn new(value: impl AsRef<str>) -> Result<Self> {
        let auth_token = normalize_auth_token(value.as_ref());
        let arl = normalize_arl(&auth_token);
        if arl.is_empty() {
            bail!("для Deezer нужен cookie arl")
        }
        let jar = Arc::new(Jar::default());
        let https = Url::parse(COOKIE_URL)?;
        let http_url = Url::parse(COOKIE_URL_HTTP)?;
        let pairs = parse_cookie_pairs(&auth_token);
        if pairs.is_empty() {
            for url in [&https, &http_url] {
                jar.add_cookie_str(
                    &format!("arl={arl}; Domain=.deezer.com; Path=/; HttpOnly"),
                    url,
                );
            }
        } else {
            for (name, cookie_value) in pairs {
                for url in [&https, &http_url] {
                    jar.add_cookie_str(
                        &format!("{name}={cookie_value}; Domain=.deezer.com; Path=/; HttpOnly"),
                        url,
                    );
                }
            }
        }
        let http = Client::builder()
            .cookie_provider(jar)
            .user_agent(BROWSER_UA)
            .build()
            .context("не удалось создать HTTP-клиент Deezer")?;
        Ok(Self { http })
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, url: Url) -> Result<T> {
        let response = self
            .http
            .get(url)
            .headers(deezer_headers())
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
            .headers(deezer_headers())
            .json(&params)
            .send()
            .await
            .context("Deezer gateway не ответил")?
            .error_for_status()
            .context("Deezer gateway отклонил запрос")?
            .json::<Value>()
            .await
            .context("Deezer gateway вернул непонятный JSON")?;
        if payload.get("error").is_some_and(gateway_has_error)
            && !gateway_has_invalid_token(&payload)
        {
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
        let mut token = text_at(&user, &["results", "checkForm"])
            .context("ARL не авторизован: Deezer не выдал checkForm")?
            .to_string();
        let mut license = text_at(&user, &["results", "USER", "OPTIONS", "license_token"])
            .context("ARL не авторизован: Deezer не выдал license_token")?
            .to_string();
        let mut raw = self
            .gateway("song.getListData", &token, json!({ "SNG_IDS": [track_id] }))
            .await?;
        if gateway_has_invalid_token(&raw) {
            let refreshed = self.gateway("deezer.getUserData", "", json!({})).await?;
            token = text_at(&refreshed, &["results", "checkForm"])
                .context("Deezer не обновил checkForm")?
                .to_string();
            license = text_at(&refreshed, &["results", "USER", "OPTIONS", "license_token"])
                .context("Deezer не обновил license_token")?
                .to_string();
            raw = self
                .gateway("song.getListData", &token, json!({ "SNG_IDS": [track_id] }))
                .await?;
        }
        if gateway_has_error(&raw) {
            bail!("Deezer gateway: {}", raw["error"])
        }
        let song = raw
            .pointer("/results/data/0")
            .context("Deezer не вернул метаданные полного трека")?;
        let track_token =
            text_at(song, &["TRACK_TOKEN"]).context("Deezer не вернул TRACK_TOKEN")?;
        let mut candidates = self
            .resolve_media(&license, track_token)
            .await
            .unwrap_or_default();
        for (format, extension) in [("MP3_320", "mp3"), ("MP3_128", "mp3"), ("FLAC", "flac")] {
            if let Ok(urls) = legacy_urls(song, format) {
                candidates.extend(urls.into_iter().map(|url| (url, extension)));
            }
        }
        let mut last_error = None;
        for (media_url, extension) in candidates {
            let response = match self
                .http
                .get(media_url)
                .headers(deezer_headers())
                .send()
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    last_error = Some(error.to_string());
                    continue;
                }
            };
            if !response.status().is_success() {
                last_error = Some(format!("HTTP {}", response.status().as_u16()));
                continue;
            }
            let bytes = response
                .bytes()
                .await
                .context("не удалось прочитать полный трек Deezer")?;
            let decrypted = decrypt_audio(&bytes, track_id)?;
            let path = deezer_cache_dir().join(format!("{track_id}.{extension}"));
            write_cache(&path, &decrypted)?;
            return Ok(path);
        }
        bail!(
            "Deezer не отдал полный трек: {}",
            last_error.unwrap_or_else(|| "нет media-кандидатов".to_string())
        )
    }

    async fn resolve_media(
        &self,
        license: &str,
        track_token: &str,
    ) -> Result<Vec<(String, &'static str)>> {
        for (format, extension) in [("MP3_320", "mp3"), ("MP3_128", "mp3"), ("FLAC", "flac")] {
            let payload = self
                .http
                .post(MEDIA)
                .headers(deezer_headers())
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
                return Ok(vec![(url.to_string(), extension)]);
            }
        }
        Ok(Vec::new())
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

fn gateway_has_invalid_token(payload: &Value) -> bool {
    let Some(errors) = payload.get("error").and_then(Value::as_object) else {
        return false;
    };
    let gateway = errors
        .get("GATEWAY_ERROR")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let required = errors.get("VALID_TOKEN_REQUIRED");
    gateway.eq_ignore_ascii_case("invalid api token")
        || required.and_then(Value::as_bool).unwrap_or(false)
        || required.and_then(Value::as_str).is_some_and(|value| {
            value.eq_ignore_ascii_case("true")
                || value.to_ascii_lowercase().contains("invalid csrf token")
        })
}

fn normalize_auth_token(value: &str) -> String {
    let value = value.trim().trim_matches(['\'', '"']).trim();
    value
        .split_once(':')
        .filter(|(prefix, _)| prefix.trim().eq_ignore_ascii_case("cookie"))
        .map_or_else(|| value.to_string(), |(_, rest)| rest.trim().to_string())
}

fn parse_cookie_pairs(value: &str) -> Vec<(String, String)> {
    normalize_auth_token(value)
        .split(';')
        .filter_map(|segment| {
            let (name, value) = segment.trim().split_once('=')?;
            let name = name.trim();
            if name.is_empty()
                || matches!(
                    name.to_ascii_lowercase().as_str(),
                    "domain" | "path" | "expires" | "max-age" | "samesite" | "secure" | "httponly"
                )
            {
                return None;
            }
            let value = value.trim().trim_matches(['\'', '"']).trim();
            (!value.is_empty()).then(|| (name.to_string(), value.to_string()))
        })
        .collect()
}

fn deezer_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(BROWSER_UA));
    headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
    headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.9"));
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers
}

fn text_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    path.iter()
        .try_fold(value, |current, key| current.get(*key))?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn legacy_urls(raw: &Value, format: &str) -> Result<Vec<String>> {
    let track_id = raw_value(raw, "SNG_ID");
    let md5_origin = raw_value(raw, "MD5_ORIGIN");
    let media_version = raw_value(raw, "MEDIA_VERSION");
    let format_code = match format {
        "MP3_128" => "1",
        "MP3_320" => "3",
        "FLAC" => "9",
        _ => bail!("неподдерживаемый Deezer legacy format: {format}"),
    };
    if track_id.is_empty() || md5_origin.is_empty() || md5_origin == "0" || media_version.is_empty()
    {
        bail!("не хватает legacy-метаданных Deezer")
    }
    [b'.', 0]
        .into_iter()
        .map(|padding| legacy_url(&track_id, &md5_origin, &media_version, format_code, padding))
        .collect()
}

fn raw_value(raw: &Value, key: &str) -> String {
    raw.get(key)
        .or_else(|| raw.get("FALLBACK").and_then(|fallback| fallback.get(key)))
        .map(|value| match value {
            Value::String(value) => value.clone(),
            _ => value.to_string(),
        })
        .unwrap_or_default()
}

fn legacy_url(
    track_id: &str,
    md5_origin: &str,
    media_version: &str,
    format_code: &str,
    padding: u8,
) -> Result<String> {
    let mut step = Vec::new();
    for (index, value) in [md5_origin, format_code, track_id, media_version]
        .into_iter()
        .enumerate()
    {
        if index > 0 {
            step.push(0xa4);
        }
        step.extend_from_slice(value.as_bytes());
    }
    let checksum = format!("{:x}", md5::compute(&step));
    let mut encrypted = Vec::new();
    encrypted.extend_from_slice(checksum.as_bytes());
    encrypted.push(0xa4);
    encrypted.extend_from_slice(&step);
    encrypted.push(0xa4);
    let remainder = encrypted.len() % 16;
    let padding_len = if remainder == 0 {
        if padding == b'.' { 16 } else { 0 }
    } else {
        16 - remainder
    };
    encrypted.extend(std::iter::repeat_n(padding, padding_len));
    let cipher = Aes128::new(GenericArray::from_slice(LEGACY_AES_KEY));
    for chunk in encrypted.chunks_exact_mut(16) {
        cipher.encrypt_block(GenericArray::from_mut_slice(chunk));
    }
    let shard = md5_origin.chars().next().context("нет Deezer CDN shard")?;
    Ok(format!(
        "https://e-cdns-proxy-{shard}.dzcdn.net/mobile/1/{}",
        hex::encode(encrypted)
    ))
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
    fn pc_cookie_parser_keeps_all_auth_cookies_and_drops_attributes() {
        assert_eq!(
            parse_cookie_pairs("arl=token; sid=session; Path=/; HttpOnly"),
            vec![
                ("arl".to_string(), "token".to_string()),
                ("sid".to_string(), "session".to_string())
            ]
        );
    }

    #[test]
    fn pc_gateway_headers_use_browser_identity() {
        let headers = deezer_headers();
        assert_eq!(headers.get(reqwest::header::ACCEPT).unwrap(), "*/*");
        assert!(
            headers
                .get(USER_AGENT)
                .unwrap()
                .to_str()
                .unwrap()
                .contains("Mozilla/5.0")
        );
    }

    #[test]
    fn pc_invalid_token_detection_triggers_session_refresh() {
        assert!(gateway_has_invalid_token(
            &json!({"error": {"GATEWAY_ERROR": "Invalid api token"}})
        ));
        assert!(gateway_has_invalid_token(
            &json!({"error": {"VALID_TOKEN_REQUIRED": true}})
        ));
        assert!(!gateway_has_invalid_token(&json!({"error": []})));
    }

    #[test]
    fn pc_legacy_cdn_fallback_builds_encrypted_mobile_url() {
        let raw = json!({
            "SNG_ID": "3135556",
            "MD5_ORIGIN": "9e5e7f6b5f5a6ec7f10f23f606b3c2ec",
            "MEDIA_VERSION": "4",
            "FILESIZE_MP3_128": "123"
        });
        let urls = legacy_urls(&raw, "MP3_128").unwrap();
        assert_eq!(urls.len(), 2);
        assert!(urls.iter().all(|url| url.contains("dzcdn.net/mobile/1/")));
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
