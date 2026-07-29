use url::Url;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum YandexPlaylistRef {
    Numeric { user_id: u64, kind: u32 },
    Named { login: String, kind: u32 },
    Uuid(String),
}

pub fn parse_playlist_url(url: &Url) -> Option<YandexPlaylistRef> {
    let host = url.host_str()?.to_ascii_lowercase();
    if host != "music.yandex.ru" && !host.ends_with(".music.yandex.ru") {
        return None;
    }
    let parts = url
        .path_segments()?
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    match parts.as_slice() {
        ["users", owner, "playlists", kind, ..] => {
            let kind = kind.parse().ok()?;
            if let Ok(user_id) = owner.parse() {
                Some(YandexPlaylistRef::Numeric { user_id, kind })
            } else {
                Some(YandexPlaylistRef::Named {
                    login: owner.to_string(),
                    kind,
                })
            }
        }
        ["playlists", uuid, ..] if !uuid.trim().is_empty() => {
            Some(YandexPlaylistRef::Uuid(uuid.to_string()))
        }
        _ => None,
    }
}
