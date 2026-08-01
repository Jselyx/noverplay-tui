use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use futures_util::{StreamExt, stream::FuturesUnordered};
use url::Url;

use crate::model::{PlaybackSource, Playlist, ProviderKind, SearchProvider, TrackRef};

pub mod deezer;
pub mod soundcloud;
pub mod yandex;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SearchPage {
    pub tracks: Vec<TrackRef>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImportedPlaylist {
    pub title: String,
    pub description: String,
    pub source_url: Url,
    pub tracks: Vec<TrackRef>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Attribution {
    pub label: String,
    pub url: Url,
}

#[async_trait]
pub trait MusicProvider: Send + Sync {
    fn kind(&self) -> ProviderKind;

    fn attribution(&self) -> Attribution;

    async fn search(&self, query: &str, cursor: Option<&str>) -> Result<SearchPage>;

    async fn import_playlist(&self, url: &Url) -> Result<ImportedPlaylist>;

    async fn related(&self, track: &TrackRef, limit: usize) -> Result<Vec<TrackRef>>;

    async fn personal_wave(&self, _limit: usize) -> Result<Vec<TrackRef>> {
        Ok(Vec::new())
    }

    async fn playback_source(&self, track: &TrackRef) -> Result<PlaybackSource>;
}

#[derive(Default)]
pub struct ProviderRegistry {
    providers: HashMap<ProviderKind, Arc<dyn MusicProvider>>,
}

impl ProviderRegistry {
    pub fn register<P>(&mut self, provider: P)
    where
        P: MusicProvider + 'static,
    {
        self.providers.insert(provider.kind(), Arc::new(provider));
    }

    pub fn get(&self, kind: ProviderKind) -> Option<Arc<dyn MusicProvider>> {
        self.providers.get(&kind).cloned()
    }

    pub fn kinds(&self) -> impl Iterator<Item = ProviderKind> + '_ {
        self.providers.keys().copied()
    }

    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    pub async fn search_all(&self, query: &str) -> Vec<(ProviderKind, Result<SearchPage>)> {
        self.search(query, SearchProvider::All).await
    }

    pub async fn search(
        &self,
        query: &str,
        selection: SearchProvider,
    ) -> Vec<(ProviderKind, Result<SearchPage>)> {
        let mut pending = FuturesUnordered::new();
        for provider in self.providers.values().filter(|provider| {
            selection
                .provider()
                .is_none_or(|kind| kind == provider.kind())
        }) {
            let query = query.to_string();
            pending.push(async move {
                let kind = provider.kind();
                (kind, provider.search(&query, None).await)
            });
        }
        let mut pages = Vec::with_capacity(self.providers.len());
        while let Some(page) = pending.next().await {
            pages.push(page);
        }
        pages.sort_by_key(|(kind, _)| provider_order(*kind));
        pages
    }

    pub async fn import_url(&self, url: &Url, now_ms: i64) -> Result<Playlist> {
        let kind = ProviderKind::from_url(url.as_str())
            .context("ссылка не похожа на SoundCloud, Yandex Music или Deezer")?;
        let provider = self
            .get(kind)
            .with_context(|| format!("провайдер {} не настроен", kind.label()))?;
        let imported = provider.import_playlist(url).await?;
        let mut playlist = Playlist::new(imported.title, now_ms);
        playlist.description = imported.description;
        playlist.source_url = Some(imported.source_url);

        // Дубли хотели пролезть вдвоём по одному паспорту, но фейс-контроль сегодня не бухой АХАХАХА 🫩
        for track in imported.tracks {
            playlist.push_unique(track);
        }
        if playlist.tracks.is_empty() {
            bail!("в плейлисте не найдено доступных треков");
        }
        Ok(playlist)
    }
}

fn provider_order(kind: ProviderKind) -> u8 {
    match kind {
        ProviderKind::SoundCloud => 0,
        ProviderKind::YandexMusic => 1,
        ProviderKind::Deezer => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PlaybackCapability;

    struct FakeProvider {
        kind: ProviderKind,
        tracks: Vec<TrackRef>,
    }

    #[async_trait]
    impl MusicProvider for FakeProvider {
        fn kind(&self) -> ProviderKind {
            self.kind
        }

        fn attribution(&self) -> Attribution {
            Attribution {
                label: self.kind.label().to_string(),
                url: Url::parse("https://example.com").unwrap(),
            }
        }

        async fn search(&self, _query: &str, _cursor: Option<&str>) -> Result<SearchPage> {
            Ok(SearchPage {
                tracks: self.tracks.clone(),
                next_cursor: None,
            })
        }

        async fn import_playlist(&self, url: &Url) -> Result<ImportedPlaylist> {
            Ok(ImportedPlaylist {
                title: "Импорт".to_string(),
                description: String::new(),
                source_url: url.clone(),
                tracks: self.tracks.clone(),
            })
        }

        async fn related(&self, _track: &TrackRef, _limit: usize) -> Result<Vec<TrackRef>> {
            Ok(self.tracks.clone())
        }

        async fn playback_source(&self, _track: &TrackRef) -> Result<PlaybackSource> {
            bail!("не нужен в этом тесте")
        }
    }

    fn track(provider: ProviderKind, id: &str) -> TrackRef {
        TrackRef {
            provider,
            id: id.to_string(),
            title: "Один трек".to_string(),
            artists: vec!["Артист".to_string()],
            duration_ms: Some(1_000),
            artwork_url: None,
            web_url: Url::parse("https://example.com/track").unwrap(),
            capability: PlaybackCapability::Full,
            genres: Vec::new(),
            explicit: false,
            drm: false,
        }
    }

    #[tokio::test]
    async fn import_removes_cross_provider_duplicates() {
        let mut registry = ProviderRegistry::default();
        registry.register(FakeProvider {
            kind: ProviderKind::Deezer,
            tracks: vec![
                track(ProviderKind::Deezer, "1"),
                track(ProviderKind::SoundCloud, "2"),
            ],
        });
        let playlist = registry
            .import_url(&Url::parse("https://deezer.com/playlist/1").unwrap(), 42)
            .await
            .unwrap();
        assert_eq!(playlist.tracks.len(), 1);
        assert_eq!(playlist.created_at_ms, 42);
    }

    #[tokio::test]
    async fn search_results_have_stable_provider_order() {
        let mut registry = ProviderRegistry::default();
        registry.register(FakeProvider {
            kind: ProviderKind::Deezer,
            tracks: vec![track(ProviderKind::Deezer, "1")],
        });
        registry.register(FakeProvider {
            kind: ProviderKind::SoundCloud,
            tracks: vec![track(ProviderKind::SoundCloud, "2")],
        });
        let results = registry.search_all("трек").await;
        assert_eq!(results[0].0, ProviderKind::SoundCloud);
        assert_eq!(results[1].0, ProviderKind::Deezer);
    }

    #[tokio::test]
    async fn provider_without_personal_radio_returns_empty_batch() {
        let provider = FakeProvider {
            kind: ProviderKind::SoundCloud,
            tracks: Vec::new(),
        };
        assert!(provider.personal_wave(40).await.unwrap().is_empty());
    }
}
