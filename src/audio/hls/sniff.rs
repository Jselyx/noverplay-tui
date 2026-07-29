use anyhow::{Result, bail};

use super::manifest::HlsAsset;

pub(super) fn sniff_extension(bytes: &[u8], asset: &HlsAsset) -> Result<&'static str> {
    if bytes.len() >= 8 && matches!(&bytes[4..8], b"ftyp" | b"styp" | b"moof") {
        return Ok("m4a");
    }
    if looks_like_transport_stream(bytes) {
        bail!("MPEG-TS HLS пока не поддерживается аудиодекодером")
    }
    if bytes.starts_with(b"OggS") {
        return Ok("ogg");
    }
    if bytes.starts_with(b"fLaC") {
        return Ok("flac");
    }
    if let Some(extension) = extension_from_url(asset) {
        return Ok(extension);
    }
    if looks_like_adts(bytes) {
        return Ok("aac");
    }
    if looks_like_mpeg_audio(bytes) || bytes.starts_with(b"ID3") {
        return Ok("mp3");
    }
    // Формат притворился пакетом из пятёрочки, но аудио внутри угадать всё равно надо
    bail!("формат HLS-сегментов не распознан")
}

fn extension_from_url(asset: &HlsAsset) -> Option<&'static str> {
    let extension = asset
        .url
        .path_segments()?
        .next_back()?
        .rsplit_once('.')?
        .1
        .to_ascii_lowercase();
    match extension.as_str() {
        "aac" => Some("aac"),
        "m4a" | "m4s" | "mp4" => Some("m4a"),
        "mp3" => Some("mp3"),
        "ogg" | "opus" => Some("ogg"),
        "flac" => Some("flac"),
        _ => None,
    }
}

fn looks_like_transport_stream(bytes: &[u8]) -> bool {
    bytes.first() == Some(&0x47) && (bytes.len() <= 188 || bytes.get(188) == Some(&0x47))
}

fn looks_like_adts(bytes: &[u8]) -> bool {
    bytes.len() >= 2 && bytes[0] == 0xff && bytes[1] & 0xf6 == 0xf0
}

fn looks_like_mpeg_audio(bytes: &[u8]) -> bool {
    bytes.len() >= 2 && bytes[0] == 0xff && bytes[1] & 0xe0 == 0xe0 && bytes[1] & 0x06 != 0
}
