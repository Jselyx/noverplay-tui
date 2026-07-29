use std::collections::HashSet;

use crate::model::TrackRef;

use super::{WaveMode, WaveSettings, WaveTasteProfile, artist_id, track_key};

pub(crate) struct WaveSeeds {
    pub tracks: Vec<TrackRef>,
    pub artist_names: Vec<String>,
    pub strict_artist_ids: HashSet<String>,
}

pub(crate) fn build_seeds(
    profile: &WaveTasteProfile,
    liked: &[(TrackRef, i64)],
    manual: &[TrackRef],
    manual_only: bool,
    settings: &WaveSettings,
) -> WaveSeeds {
    let tracks = merge_tracks(manual, &profile.recent_tracks, manual_only);
    let mut artist_ids = manual.iter().map(artist_id).collect::<Vec<_>>();
    if !manual_only {
        if matches!(settings.mode, WaveMode::Balanced | WaveMode::Radio) {
            artist_ids.extend(profile.recent_artist_ids.iter().cloned());
            artist_ids.extend(profile.recent_top_artists.iter().map(|(id, _)| id.clone()));
            artist_ids.extend(
                profile
                    .liked_top_artists_recent
                    .iter()
                    .map(|(id, _)| id.clone()),
            );
        } else {
            artist_ids.extend(profile.all_top_artists.iter().map(|(id, _)| id.clone()));
            artist_ids.extend(profile.recent_top_artists.iter().map(|(id, _)| id.clone()));
            artist_ids.extend(
                profile
                    .liked_top_artists_alltime
                    .iter()
                    .map(|(id, _)| id.clone()),
            );
        }
    }
    let artist_ids = unique(artist_ids, 8);
    let mut artist_names = unique_names(manual.iter().map(TrackRef::display_artist), 8);
    for id in &artist_ids {
        if let Some(name) = profile.artist_names.get(id)
            && !artist_names
                .iter()
                .any(|current| current.eq_ignore_ascii_case(name))
        {
            artist_names.push(name.clone());
        }
    }
    if profile.recent_tracks.is_empty() && manual.is_empty() {
        for (track, _) in liked.iter().take(5) {
            let name = track.display_artist();
            if !name.trim().is_empty()
                && !artist_names
                    .iter()
                    .any(|current| current.eq_ignore_ascii_case(&name))
            {
                artist_names.push(name);
            }
        }
    }
    if artist_names.is_empty() {
        artist_names.push("popular music".to_string());
    }
    let strict_artist_ids = if settings.novelty <= 0.001 {
        let source = if manual.is_empty() {
            tracks.iter().take(8).collect::<Vec<_>>()
        } else {
            manual.iter().collect::<Vec<_>>()
        };
        source.into_iter().map(artist_id).collect()
    } else {
        HashSet::new()
    };
    WaveSeeds {
        tracks,
        artist_names,
        strict_artist_ids,
    }
}

fn merge_tracks(manual: &[TrackRef], recent: &[TrackRef], manual_only: bool) -> Vec<TrackRef> {
    let source: Box<dyn Iterator<Item = &TrackRef>> = if manual_only {
        Box::new(manual.iter())
    } else {
        Box::new(manual.iter().chain(recent))
    };
    let mut seen = HashSet::new();
    source
        .filter_map(|track| {
            if seen.insert(track_key(track)) {
                Some(track.clone())
            } else {
                None
            }
        })
        .collect()
}

fn unique(values: Vec<String>, limit: usize) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter(|value| !value.trim().is_empty() && seen.insert(value.clone()))
        .take(limit)
        .collect()
}

fn unique_names(values: impl Iterator<Item = String>, limit: usize) -> Vec<String> {
    let mut result = Vec::<String>::new();
    for value in values {
        if value.trim().is_empty()
            || result
                .iter()
                .any(|current| current.eq_ignore_ascii_case(&value))
        {
            continue;
        }
        result.push(value);
        if result.len() >= limit {
            break;
        }
    }
    result
}
