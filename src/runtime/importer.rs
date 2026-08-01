use std::{sync::Arc, time::SystemTime};

use anyhow::{Context, Result};
use tokio::{sync::mpsc::UnboundedSender, task::JoinHandle};
use url::Url;

use crate::{model::ProviderKind, provider::ProviderRegistry, storage::Storage};

use super::message::RuntimeMessage;

pub(super) fn spawn_import(
    providers: Arc<ProviderRegistry>,
    storage: Storage,
    sender: UnboundedSender<RuntimeMessage>,
    generation: u64,
    source: String,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let result = import_playlist(&providers, &storage, &source)
            .await
            .map_err(|error| format!("{error:#}"));
        let _ = sender.send(RuntimeMessage::PlaylistImported { generation, result });
    })
}

async fn import_playlist(
    providers: &ProviderRegistry,
    storage: &Storage,
    source: &str,
) -> Result<crate::model::Playlist> {
    let url = Url::parse(source.trim()).context("ссылка повреждена")?;
    let _provider_kind = ProviderKind::from_url(url.as_str());
    let now_ms = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default();
    let playlist = providers.import_url(&url, now_ms).await?;
    storage
        .save_playlist(&playlist)
        .context("не удалось сохранить плейлист")?;
    Ok(playlist)
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, bail};
    use async_trait::async_trait;

    use crate::{
        model::{PlaybackCapability, PlaybackSource, TrackRef},
        provider::{Attribution, ImportedPlaylist, MusicProvider, SearchPage},
    };

    use super::*;

    struct FakeSoundCloud;

    #[async_trait]
    impl MusicProvider for FakeSoundCloud {
        fn kind(&self) -> ProviderKind {
            ProviderKind::SoundCloud
        }

        fn attribution(&self) -> Attribution {
            Attribution {
                label: "mock".to_string(),
                url: Url::parse("https://soundcloud.com").unwrap(),
            }
        }

        async fn search(&self, _query: &str, _cursor: Option<&str>) -> Result<SearchPage> {
            Ok(SearchPage::default())
        }

        async fn import_playlist(&self, url: &Url) -> Result<ImportedPlaylist> {
            Ok(ImportedPlaylist {
                title: "Импорт".to_string(),
                description: String::new(),
                source_url: url.clone(),
                tracks: vec![track()],
            })
        }

        async fn related(&self, _track: &TrackRef, _limit: usize) -> Result<Vec<TrackRef>> {
            Ok(Vec::new())
        }

        async fn playback_source(&self, _track: &TrackRef) -> Result<PlaybackSource> {
            bail!("не нужен")
        }
    }

    #[tokio::test]
    async fn import_uses_provider_and_lands_in_sqlite() {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path().join("library.sqlite3"));
        storage.initialize().unwrap();
        let mut providers = ProviderRegistry::default();
        providers.register(FakeSoundCloud);

        let playlist =
            import_playlist(&providers, &storage, "https://soundcloud.com/user/sets/mix")
                .await
                .unwrap();

        assert_eq!(playlist.tracks, vec![track()]);
        assert_eq!(storage.list_playlists().unwrap().len(), 1);
    }

    fn track() -> TrackRef {
        TrackRef {
            provider: ProviderKind::SoundCloud,
            id: "42".to_string(),
            title: "Трек".to_string(),
            artists: vec!["Автор".to_string()],
            duration_ms: Some(60_000),
            artwork_url: None,
            web_url: Url::parse("https://soundcloud.com/user/track").unwrap(),
            capability: PlaybackCapability::Full,
            genres: Vec::new(),
            explicit: false,
            drm: false,
        }
    }
}
