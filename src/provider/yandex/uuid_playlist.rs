use anyhow::{Context, Result};
use serde_json::Value;
use url::Url;
use yandex_music::{API_PATH, YandexMusicClient, model::playlist::Playlist};

pub(super) async fn load_uuid_playlist(
    client: &YandexMusicClient,
    uuid: &str,
    page: u32,
    page_size: u32,
) -> Result<Playlist> {
    let mut url = Url::parse(API_PATH).context("неверный адрес API Yandex Music")?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("адрес API Yandex Music нельзя изменить"))?
        .extend(["playlist", uuid]);
    url.query_pairs_mut()
        .append_pair("page", &page.to_string())
        .append_pair("pageSize", &page_size.to_string())
        .append_pair("richTracks", "true");
    let payload: Value = client
        .inner
        .get(url)
        .send()
        .await
        .context("Yandex Music не ответил на UUID плейлиста")?
        .error_for_status()
        .context("Yandex Music отклонил UUID плейлиста")?
        .json()
        .await
        .context("Yandex Music вернул непонятный UUID плейлист")?;
    let result = payload.get("result").cloned().unwrap_or(payload);
    serde_json::from_value(result).context("не удалось разобрать UUID плейлист Yandex Music")
}
