use anyhow::{Context, Result};
use url::Url;
use yandex_music::{
    YandexMusicClient, api::playlist::get_playlist::GetPlaylistOptions, model::playlist::Playlist,
};

use crate::provider::ImportedPlaylist;

use super::{
    YandexPlaylistRef, parse_playlist_url, playlist_tracks::zagruzit_treki_pleilista,
    user::resolve_user_id, uuid_playlist::load_uuid_playlist,
};

const PAGE_SIZE: u32 = 200;
const MAX_PAGES: u32 = 100;

pub(super) async fn import_playlist(
    client: &YandexMusicClient,
    source_url: &Url,
) -> Result<ImportedPlaylist> {
    let playlist_ref = parse_playlist_url(source_url)
        .context("не удалось разобрать ссылку на плейлист Yandex Music")?;
    let resolved = resolve_owner(client, playlist_ref).await?;
    let mut first = load_page(client, &resolved, 1).await?;
    let title = first.title.clone();
    let description = first
        .description_formatted
        .clone()
        .or_else(|| first.description.clone())
        .unwrap_or_default();
    let expected = first.track_count as usize;
    let mut tracks = zagruzit_treki_pleilista(client, first.tracks.take()).await?;

    let mut page = 2;
    while tracks.len() < expected && page <= MAX_PAGES {
        let mut next = load_page(client, &resolved, page).await?;
        let loaded = zagruzit_treki_pleilista(client, next.tracks.take()).await?;
        if loaded.is_empty() {
            break;
        }
        tracks.extend(loaded);
        page += 1;
    }

    // Сто страниц это уже не плейлист, а личное дело на весь архив, дальше без фанатизма))))
    Ok(ImportedPlaylist {
        title,
        description,
        source_url: source_url.clone(),
        tracks,
    })
}

async fn resolve_owner(
    client: &YandexMusicClient,
    playlist_ref: YandexPlaylistRef,
) -> Result<YandexPlaylistRef> {
    match playlist_ref {
        YandexPlaylistRef::Named { login, kind } => Ok(YandexPlaylistRef::Numeric {
            user_id: resolve_user_id(client, &login).await?,
            kind,
        }),
        other => Ok(other),
    }
}

async fn load_page(
    client: &YandexMusicClient,
    playlist_ref: &YandexPlaylistRef,
    page: u32,
) -> Result<Playlist> {
    match playlist_ref {
        YandexPlaylistRef::Numeric { user_id, kind } => client
            .get_playlist(
                &GetPlaylistOptions::new(*user_id, *kind)
                    .page(page)
                    .page_size(PAGE_SIZE)
                    .rich_tracks(true),
            )
            .await
            .context("Yandex Music не отдал страницу плейлиста"),
        YandexPlaylistRef::Uuid(uuid) => load_uuid_playlist(client, uuid, page, PAGE_SIZE).await,
        YandexPlaylistRef::Named { .. } => unreachable!("логин владельца уже разрешен"),
    }
}
