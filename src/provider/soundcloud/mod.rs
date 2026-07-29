mod client;
mod mapping;
mod models;
mod playback;
mod playlist;
mod related;
mod search;
mod source_url;
mod track_details;

use anyhow::Result;
use async_trait::async_trait;
use url::Url;

use crate::{
    model::{PlaybackSource, ProviderKind, TrackRef},
    provider::{Attribution, ImportedPlaylist, MusicProvider, SearchPage},
};

use client::SoundCloudClient;
use playback::poluchit_istochnik;
use playlist::import_playlist;
use related::related_tracks;
use search::search_tracks;

pub use mapping::normalizovat_track;
pub use models::{ScCollection, ScPlaylist, ScTrack};
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

    async fn import_playlist(&self, url: &Url) -> Result<ImportedPlaylist> {
        import_playlist(&self.client, url).await
    }

    async fn related(&self, track: &TrackRef, limit: usize) -> Result<Vec<TrackRef>> {
        related_tracks(&self.client, track, limit).await
    }

    async fn playback_source(&self, track: &TrackRef) -> Result<PlaybackSource> {
        poluchit_istochnik(&self.client, track).await
    }
}

#[cfg(test)]
mod tests;
