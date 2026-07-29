use std::collections::HashMap;

use anyhow::{Context, Result};

use super::{client::SoundCloudClient, models::ScTrack};

const TRACKS_PER_REQUEST: usize = 50;

pub(super) async fn zagruzit_dannye_trekov(
    client: &SoundCloudClient,
    tracks: Vec<ScTrack>,
) -> Result<Vec<ScTrack>> {
    let missing_ids = tracks
        .iter()
        .filter(|track| needs_details(track))
        .map(|track| track.id.clone())
        .collect::<Vec<_>>();
    if missing_ids.is_empty() {
        return Ok(tracks);
    }

    let mut details = HashMap::new();
    for chunk in missing_ids.chunks(TRACKS_PER_REQUEST) {
        let loaded: Vec<ScTrack> = client
            .get_json(client.v2_url(&["tracks"])?, &[("ids", chunk.join(","))])
            .await
            .context("SoundCloud не раскрыл сокращенные треки плейлиста")?;
        details.extend(loaded.into_iter().map(|track| (track.id.clone(), track)));
    }

    // Порядок плейлиста святой, даже если api вывалил ответы как носки после этапа
    Ok(tracks
        .into_iter()
        .map(|track| details.get(&track.id).cloned().unwrap_or(track))
        .collect())
}

fn needs_details(track: &ScTrack) -> bool {
    track.title.trim().is_empty()
        || track.user.username.trim().is_empty()
        || track.permalink_url.is_none()
}
