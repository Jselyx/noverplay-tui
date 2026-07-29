use std::collections::BTreeMap;

use anyhow::{Context, Result, bail, ensure};
use url::Url;

const ALLOWED_AUDIO_HOSTS: &[&str] = &["sndcdn.com", "soundcloud.com", "soundcloud.cloud"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct HlsByteRange {
    pub(super) start: u64,
    pub(super) length: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct HlsAsset {
    pub(super) url: Url,
    pub(super) byte_range: Option<HlsByteRange>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct HlsSegment {
    pub(super) asset: HlsAsset,
    pub(super) duration_ms: u64,
    pub(super) init: Option<HlsAsset>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct HlsMediaPlaylist {
    pub(super) segments: Vec<HlsSegment>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct HlsVariant {
    pub(super) url: Url,
    pub(super) bandwidth: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum HlsPlaylist {
    Media(HlsMediaPlaylist),
    Master(Vec<HlsVariant>),
}

pub(super) fn parse_playlist(base_url: &Url, body: &str) -> Result<HlsPlaylist> {
    ensure!(
        body.trim_start().starts_with("#EXTM3U"),
        "ответ не похож на HLS-плейлист"
    );

    let mut segments = Vec::new();
    let mut variants = Vec::new();
    let mut duration_ms = None;
    let mut segment_range = None;
    let mut current_init = None;
    let mut pending_variant = None;
    let mut previous_range = None;

    for raw_line in body.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line == "#EXTM3U" {
            continue;
        }
        if let Some(value) = line.strip_prefix("#EXT-X-KEY:") {
            reject_encryption(value)?;
            continue;
        }
        if let Some(value) = line.strip_prefix("#EXT-X-SESSION-KEY:") {
            reject_encryption(value)?;
            continue;
        }
        if let Some(value) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            let attributes = parse_attributes(value)?;
            pending_variant = Some(
                attributes
                    .get("BANDWIDTH")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or_default(),
            );
            continue;
        }
        if let Some(value) = line.strip_prefix("#EXT-X-MAP:") {
            let attributes = parse_attributes(value)?;
            let uri = attributes.get("URI").context("в EXT-X-MAP нет URI")?;
            let url = resolve_url(base_url, uri)?;
            let byte_range = attributes
                .get("BYTERANGE")
                .map(|value| parse_byte_range(value, None, &url))
                .transpose()?;
            current_init = Some(HlsAsset { url, byte_range });
            continue;
        }
        if let Some(value) = line.strip_prefix("#EXT-X-BYTERANGE:") {
            segment_range = Some(value.trim().to_string());
            continue;
        }
        if let Some(value) = line.strip_prefix("#EXTINF:") {
            let seconds = value
                .split(',')
                .next()
                .context("у EXTINF пропала длительность")?
                .trim()
                .parse::<f64>()
                .context("у EXTINF сломана длительность")?;
            ensure!(
                seconds.is_finite() && seconds >= 0.0,
                "неверная длительность HLS-сегмента"
            );
            duration_ms = Some((seconds * 1000.0).round() as u64);
            continue;
        }
        if line.starts_with('#') {
            continue;
        }

        let url = resolve_url(base_url, line)?;
        if let Some(bandwidth) = pending_variant.take() {
            variants.push(HlsVariant { url, bandwidth });
            continue;
        }

        let byte_range = segment_range
            .take()
            .map(|value| parse_byte_range(&value, previous_range.as_ref(), &url))
            .transpose()?;
        if let Some(range) = &byte_range {
            previous_range = Some((url.clone(), range.start.saturating_add(range.length)));
        } else {
            previous_range = None;
        }
        segments.push(HlsSegment {
            asset: HlsAsset { url, byte_range },
            duration_ms: duration_ms.take().unwrap_or_default(),
            init: current_init.clone(),
        });
    }

    if !variants.is_empty() {
        ensure!(
            segments.is_empty(),
            "HLS смешал master и media в одну баланду"
        );
        return Ok(HlsPlaylist::Master(variants));
    }
    ensure!(!segments.is_empty(), "в HLS-плейлисте нет аудиосегментов");
    Ok(HlsPlaylist::Media(HlsMediaPlaylist { segments }))
}

fn reject_encryption(value: &str) -> Result<()> {
    let attributes = parse_attributes(value)?;
    let method = attributes.get("METHOD").map(String::as_str).unwrap_or("");
    if method.eq_ignore_ascii_case("NONE") {
        return Ok(());
    }
    // Если тут ключ, значит музыка уже пришла с наручниками, вскрывать их клиент не будет
    bail!("зашифрованный HLS не поддерживается")
}

fn resolve_url(base_url: &Url, value: &str) -> Result<Url> {
    let url = base_url
        .join(value.trim())
        .context("в HLS-плейлисте неверный адрес")?;
    validate_url(&url)?;
    Ok(url)
}

pub(super) fn validate_url(url: &Url) -> Result<()> {
    let safe_local_test =
        url.scheme() == "http" && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1"));
    let allowed_soundcloud = url.scheme() == "https"
        && url
            .host_str()
            .is_some_and(|host| has_allowed_host(host, ALLOWED_AUDIO_HOSTS));
    ensure!(
        allowed_soundcloud || safe_local_test,
        "HLS пытается уйти на чужой или небезопасный адрес"
    );
    ensure!(url.host_str().is_some(), "у HLS-адреса нет хоста");
    ensure!(
        url.username().is_empty() && url.password().is_none(),
        "в HLS-адресе зачем-то лежат логин и пароль"
    );
    Ok(())
}

fn has_allowed_host(host: &str, suffixes: &[&str]) -> bool {
    let host = host.trim().to_ascii_lowercase();
    suffixes
        .iter()
        .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
}

fn parse_byte_range(value: &str, previous: Option<&(Url, u64)>, url: &Url) -> Result<HlsByteRange> {
    let (length, explicit_start) = value
        .trim_matches('"')
        .split_once('@')
        .map_or((value.trim_matches('"'), None), |(length, start)| {
            (length, Some(start))
        });
    let length = length
        .parse::<u64>()
        .context("сломана длина EXT-X-BYTERANGE")?;
    ensure!(length > 0, "пустой EXT-X-BYTERANGE");
    let start = if let Some(start) = explicit_start {
        start.parse().context("сломан offset EXT-X-BYTERANGE")?
    } else if let Some((_, end)) = previous.filter(|(previous_url, _)| previous_url == url) {
        *end
    } else {
        bail!("у EXT-X-BYTERANGE без offset нет предыдущего куска")
    };
    Ok(HlsByteRange { start, length })
}

fn parse_attributes(value: &str) -> Result<BTreeMap<String, String>> {
    let mut attributes = BTreeMap::new();
    let mut start = 0;
    let mut quoted = false;
    let bytes = value.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        match byte {
            b'"' => quoted = !quoted,
            b',' if !quoted => {
                insert_attribute(&mut attributes, &value[start..index])?;
                start = index + 1;
            }
            _ => {}
        }
    }
    ensure!(!quoted, "в атрибутах HLS не закрыта кавычка");
    insert_attribute(&mut attributes, &value[start..])?;
    Ok(attributes)
}

fn insert_attribute(attributes: &mut BTreeMap<String, String>, raw: &str) -> Result<()> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(());
    }
    let (name, value) = raw.split_once('=').context("сломанный атрибут HLS")?;
    attributes.insert(
        name.trim().to_ascii_uppercase(),
        value.trim().trim_matches('"').to_string(),
    );
    Ok(())
}
