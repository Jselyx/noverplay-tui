use anyhow::{Context, Result};
use yandex_music::{
    YandexMusicClient, api::track::get_tracks::GetTracksOptions, model::playlist::PlaylistTracks,
};

use crate::model::TrackRef;

use super::normalizovat_track;

pub(super) async fn zagruzit_treki_pleilista(
    client: &YandexMusicClient,
    tracks: Option<PlaylistTracks>,
) -> Result<Vec<TrackRef>> {
    let Some(tracks) = tracks else {
        return Ok(Vec::new());
    };
    match tracks {
        PlaylistTracks::Full(tracks) => {
            Ok(tracks.into_iter().filter_map(normalizovat_track).collect())
        }
        PlaylistTracks::WithInfo(tracks) => Ok(tracks
            .into_iter()
            .filter_map(|entry| normalizovat_track(entry.track))
            .collect()),
        PlaylistTracks::Partial(tracks) => {
            let mut expanded = Vec::with_capacity(tracks.len());
            let ids = tracks.into_iter().map(|track| track.id).collect::<Vec<_>>();
            for chunk in ids.chunks(100) {
                let hydrated = client
                    .get_tracks(&GetTracksOptions::new(chunk.to_vec()))
                    .await
                    .context("Yandex Music не раскрыл сокращенные треки плейлиста")?;
                expanded.extend(hydrated.into_iter().filter_map(normalizovat_track));
            }
            // Сто айди за запрос, потому что грузить каждый по одному могут только особо опасные рецидивисты 🫩
            Ok(expanded)
        }
    }
}
