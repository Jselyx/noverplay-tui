use anyhow::Result;
use symphonia::core::io::MediaSource;

use crate::model::PlaybackSource;

use super::{hls::HlsSource, http_source::HttpRangeSource};

pub(super) struct OpenedMedia {
    pub(super) source: Box<dyn MediaSource>,
    pub(super) extension: Option<String>,
    pub(super) mime_type: Option<String>,
    pub(super) seek_in_format: bool,
    pub(super) discard_ms: u64,
}

pub(super) fn open_media(source: &PlaybackSource, position_ms: u64) -> Result<OpenedMedia> {
    if source.url.scheme() == "file" {
        let path = source
            .url
            .to_file_path()
            .map_err(|_| anyhow::anyhow!("повреждённый локальный аудиопуть"))?;
        let file = std::fs::File::open(&path)?;
        return Ok(OpenedMedia {
            source: Box::new(file),
            extension: path
                .extension()
                .and_then(|value| value.to_str())
                .map(str::to_string),
            mime_type: source.mime_type.clone(),
            seek_in_format: position_ms > 0,
            discard_ms: 0,
        });
    }
    if is_hls(source) {
        let hls = HlsSource::open(&source.url, &source.headers, position_ms)?;
        let discard_ms = position_ms.saturating_sub(hls.start_ms());
        let extension = Some(hls.extension().to_string());
        return Ok(OpenedMedia {
            source: Box::new(hls),
            extension,
            mime_type: None,
            seek_in_format: false,
            discard_ms,
        });
    }

    let media = HttpRangeSource::open(source.url.as_str(), &source.headers, source.supports_range)?;
    Ok(OpenedMedia {
        source: Box::new(media),
        extension: extension_from_url(source),
        mime_type: source.mime_type.clone(),
        seek_in_format: position_ms > 0,
        discard_ms: 0,
    })
}

fn is_hls(source: &PlaybackSource) -> bool {
    let mime_is_hls = source.mime_type.as_deref().is_some_and(|mime| {
        let mime = mime.to_ascii_lowercase();
        mime.contains("mpegurl") || mime.contains("x-mpegurl")
    });
    let path = source.url.path().to_ascii_lowercase();
    let last_segment = path.rsplit('/').next().unwrap_or_default();
    mime_is_hls || path.ends_with(".m3u8") || last_segment.starts_with("chunklist")
}

fn extension_from_url(source: &PlaybackSource) -> Option<String> {
    source
        .url
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .and_then(|name| {
            name.rsplit_once('.')
                .map(|(_, extension)| extension.to_string())
        })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use url::Url;

    use crate::model::{PlaybackCapability, PlaybackSource};

    use super::is_hls;

    #[test]
    fn hls_is_recognized_by_mime_or_path() {
        assert!(is_hls(&source(
            "https://cdn.example/audio",
            Some("application/vnd.apple.mpegurl")
        )));
        assert!(is_hls(&source("https://cdn.example/chunklist_42", None)));
        assert!(!is_hls(&source(
            "https://cdn.example/audio.mp3",
            Some("audio/mpeg")
        )));
    }

    fn source(url: &str, mime_type: Option<&str>) -> PlaybackSource {
        PlaybackSource {
            url: Url::parse(url).unwrap(),
            headers: BTreeMap::new(),
            mime_type: mime_type.map(str::to_string),
            supports_range: false,
            expires_at_ms: None,
            capability: PlaybackCapability::Full,
        }
    }
}
