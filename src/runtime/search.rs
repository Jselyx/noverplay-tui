use std::{collections::HashSet, sync::Arc, time::Duration};

use tokio::{sync::mpsc::UnboundedSender, task::JoinHandle, time::sleep};

use crate::{
    model::{SearchProvider, TrackRef},
    provider::{ProviderRegistry, SearchPage},
};

use super::message::RuntimeMessage;

pub(super) fn spawn_search(
    providers: Arc<ProviderRegistry>,
    sender: UnboundedSender<RuntimeMessage>,
    generation: u64,
    query: String,
    provider: SearchProvider,
    delay: Duration,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        if !delay.is_zero() {
            sleep(delay).await;
        }
        let pages = providers.search(&query, provider).await;
        let (tracks, failures) = merge_pages(pages);
        let _ = sender.send(RuntimeMessage::SearchFinished {
            generation,
            query,
            tracks,
            failures,
        });
    })
}

pub(super) fn merge_pages(
    pages: Vec<(crate::model::ProviderKind, anyhow::Result<SearchPage>)>,
) -> (Vec<TrackRef>, Vec<String>) {
    let mut tracks = Vec::new();
    let mut failures = Vec::new();
    let mut provider_keys = HashSet::new();
    let mut canonical_keys = HashSet::new();
    for (kind, page) in pages {
        match page {
            Ok(page) => {
                for track in page.tracks {
                    if provider_keys.insert(track.provider_key())
                        && canonical_keys.insert(track.canonical_key())
                    {
                        tracks.push(track);
                    }
                }
            }
            Err(error) => failures.push(provider_failure(kind, &error)),
        }
    }
    (tracks, failures)
}

fn provider_failure(kind: crate::model::ProviderKind, error: &anyhow::Error) -> String {
    let detail = format!("{error:#}");
    let lower = detail.to_ascii_lowercase();
    let credential_rejected = ["401", "403", "unauthorized", "forbidden"]
        .iter()
        .any(|marker| lower.contains(marker));
    match (kind, credential_rejected) {
        (crate::model::ProviderKind::SoundCloud, true) => {
            "SoundCloud: client_id отклонён, обнови его в Настройках".to_string()
        }
        (crate::model::ProviderKind::YandexMusic, true) => {
            "Yandex Music: OAuth токен отклонён, обнови его в Настройках".to_string()
        }
        _ => format!("{}: {detail}", kind.label()),
    }
}

#[cfg(test)]
mod tests {
    use url::Url;

    use crate::model::{PlaybackCapability, ProviderKind};

    use super::*;

    #[test]
    fn pages_keep_order_and_throw_duplicates_overboard() {
        let pages = vec![
            (
                ProviderKind::SoundCloud,
                Ok(SearchPage {
                    tracks: vec![track(ProviderKind::SoundCloud, "1")],
                    next_cursor: None,
                }),
            ),
            (
                ProviderKind::YandexMusic,
                Ok(SearchPage {
                    tracks: vec![track(ProviderKind::YandexMusic, "2")],
                    next_cursor: None,
                }),
            ),
            (ProviderKind::Deezer, Err(anyhow::anyhow!("не настроен"))),
        ];
        let (tracks, failures) = merge_pages(pages);
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].provider, ProviderKind::SoundCloud);
        assert_eq!(failures.len(), 1);
    }

    #[test]
    fn rejected_key_points_to_the_settings_instead_of_mumbling() {
        let error = anyhow::anyhow!("HTTP status client error (401 Unauthorized)")
            .context("SoundCloud отклонил запрос");

        assert_eq!(
            provider_failure(ProviderKind::SoundCloud, &error),
            "SoundCloud: client_id отклонён, обнови его в Настройках"
        );
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
}
