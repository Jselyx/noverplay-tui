mod client;
mod mapping;
mod models;
mod search;
mod source_url;

use anyhow::{Result, bail};
use async_trait::async_trait;
use url::Url;

use crate::{
    model::{PlaybackSource, ProviderKind, TrackRef},
    provider::{Attribution, ImportedPlaylist, MusicProvider, SearchPage},
};

use client::SoundCloudClient;
use search::search_tracks;

pub use mapping::normalizovat_track;
pub use models::{ScCollection, ScPlaylist, ScStreams, ScTrack};
pub use source_url::proverit_soundcloud_url;

pub struct SoundCloudProvider {
    client: SoundCloudClient,
}

impl SoundCloudProvider {
    pub fn new(client_id: impl Into<String>) -> Result<Self> {
        Ok(Self {
            client: SoundCloudClient::new(client_id.into())?,
        })
    }
}

#[async_trait]
impl MusicProvider for SoundCloudProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::SoundCloud
    }

    fn attribution(&self) -> Attribution {
        Attribution {
            label: "SoundCloud".to_string(),
            url: Url::parse("https://soundcloud.com").expect("статический адрес SoundCloud"),
        }
    }

    async fn search(&self, query: &str, cursor: Option<&str>) -> Result<SearchPage> {
        search_tracks(&self.client, query, cursor).await
    }

    async fn import_playlist(&self, _url: &Url) -> Result<ImportedPlaylist> {
        bail!("импорт SoundCloud еще не подключен")
    }

    async fn related(&self, _track: &TrackRef, _limit: usize) -> Result<Vec<TrackRef>> {
        bail!("рекомендации SoundCloud еще не подключены")
    }

    async fn playback_source(&self, _track: &TrackRef) -> Result<PlaybackSource> {
        bail!("воспроизведение SoundCloud еще не подключено")
    }
}

#[cfg(test)]
mod tests;
