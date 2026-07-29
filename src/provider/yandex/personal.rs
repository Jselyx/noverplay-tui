use std::collections::HashSet;

use anyhow::Result;
use yandex_music::{YandexMusicClient, api::rotor::get_station_tracks::GetStationTracksOptions};

use crate::model::TrackRef;

use super::normalizovat_track;

pub(super) async fn personal_wave(
    client: &YandexMusicClient,
    limit: usize,
) -> Result<Vec<TrackRef>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let target = limit.min(120);
    let mut tracks = Vec::new();
    let mut seen = HashSet::new();
    let mut queue_marker = None;
    for _ in 0..4 {
        let mut options = GetStationTracksOptions::new("user:onyourwave").settings2(true);
        if let Some(queue) = queue_marker.take() {
            options = options.queue(queue);
        }
        let Ok(batch) = client.get_station_tracks(&options).await else {
            break;
        };
        queue_marker = Some(batch.batch_id);
        if batch.sequence.is_empty() {
            break;
        }
        for item in batch.sequence {
            let Some(track) = normalizovat_track(item.track) else {
                continue;
            };
            if !seen.insert(track.provider_key()) {
                continue;
            }
            tracks.push(track);
            if tracks.len() >= target {
                return Ok(tracks);
            }
        }
    }
    Ok(tracks)
}
