use anyhow::{Context, Result, ensure};
use yandex_music::{YandexMusicClient, api::track::get_similar_tracks::GetSimilarTracksOptions};

use crate::model::{ProviderKind, TrackRef};

use super::normalizovat_track;

pub(super) async fn related_tracks(
    client: &YandexMusicClient,
    track: &TrackRef,
    limit: usize,
) -> Result<Vec<TrackRef>> {
    ensure!(
        track.provider == ProviderKind::YandexMusic,
        "трек принадлежит другому провайдеру"
    );
    if limit == 0 {
        return Ok(Vec::new());
    }
    let response = client
        .get_similar_tracks(&GetSimilarTracksOptions::new(&track.id))
        .await
        .context("Yandex Music не отдал похожие треки")?;
    Ok(response
        .similar_tracks
        .into_iter()
        .filter_map(normalizovat_track)
        .take(limit)
        .collect())
}
