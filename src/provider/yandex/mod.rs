mod client;
mod mapping;
mod playlist_url;
mod search;

use std::sync::Arc;

use anyhow::{Result, bail};
use async_trait::async_trait;
use url::Url;
use yandex_music::YandexMusicClient;

use crate::{
    model::{PlaybackSource, ProviderKind, TrackRef},
    provider::{Attribution, ImportedPlaylist, MusicProvider, SearchPage},
};

use client::build_client;
use search::search_tracks;

pub use mapping::normalizovat_track;
pub use playlist_url::{YandexPlaylistRef, parse_playlist_url};

pub struct YandexProvider {
    client: Arc<YandexMusicClient>,
}

impl YandexProvider {
    pub fn new(token: impl AsRef<str>) -> Result<Self> {
        Ok(Self {
            client: Arc::new(build_client(token.as_ref())?),
        })
    }
}

#[async_trait]
impl MusicProvider for YandexProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::YandexMusic
    }

    fn attribution(&self) -> Attribution {
        Attribution {
            label: "Yandex Music".to_string(),
            url: Url::parse("https://music.yandex.ru").expect("статический адрес Yandex Music"),
        }
    }

    async fn search(&self, query: &str, cursor: Option<&str>) -> Result<SearchPage> {
        search_tracks(&self.client, query, cursor).await
    }

    async fn import_playlist(&self, _url: &Url) -> Result<ImportedPlaylist> {
        bail!("импорт Yandex Music еще не подключен")
    }

    async fn related(&self, _track: &TrackRef, _limit: usize) -> Result<Vec<TrackRef>> {
        bail!("рекомендации Yandex Music еще не подключены")
    }

    async fn playback_source(&self, _track: &TrackRef) -> Result<PlaybackSource> {
        bail!("воспроизведение Yandex Music еще не подключено")
    }
}

#[cfg(test)]
mod tests;
