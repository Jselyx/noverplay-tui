use std::collections::{HashMap, HashSet};

use crate::{model::TrackRef, storage::HistoryEntry};

use super::{artist_id, track_key};

const DAY_MS: i64 = 86_400_000;

#[derive(Clone, Debug, Default)]
pub struct WaveTasteProfile {
    pub recent_tracks: Vec<TrackRef>,
    pub recent_track_keys: Vec<String>,
    pub recent_artist_ids: Vec<String>,
    pub recent_top_artists: Vec<(String, i64)>,
    pub all_top_artists: Vec<(String, i64)>,
    pub liked_top_artists_recent: Vec<(String, i64)>,
    pub liked_top_artists_alltime: Vec<(String, i64)>,
    pub recent_artist_map: HashMap<String, i64>,
    pub all_artist_map: HashMap<String, i64>,
    pub liked_track_keys: HashSet<String>,
    pub liked_artist_ids: HashSet<String>,
    pub artist_names: HashMap<String, String>,
    play_events: Vec<(String, i64)>,
}

impl WaveTasteProfile {
    pub fn build(history: &[HistoryEntry], liked: &[(TrackRef, i64)], now_ms: i64) -> Self {
        let mut history = history.to_vec();
        history.sort_by(|left, right| right.played_at_ms.cmp(&left.played_at_ms));
        let recent_tracks = recent_unique_tracks(&history, 20);
        let recent_track_keys = recent_tracks.iter().map(track_key).collect::<Vec<_>>();
        let recent_artist_ids = recent_unique_artists(&history, 20);
        let recent_top_artists = rank_history_artists(&history, now_ms - 14 * DAY_MS, 24);
        let all_top_artists = rank_history_artists(&history, 0, 40);
        let liked_top_artists_recent = rank_liked_artists(liked, now_ms - 30 * DAY_MS, 24);
        let liked_top_artists_alltime = rank_liked_artists(liked, 0, 40);
        let recent_artist_map = recent_top_artists.iter().cloned().collect();
        let all_artist_map = all_top_artists.iter().cloned().collect();
        let liked_track_keys = liked.iter().map(|(track, _)| track_key(track)).collect();
        let liked_artist_ids = liked.iter().map(|(track, _)| artist_id(track)).collect();
        let mut artist_names = HashMap::new();
        for track in history
            .iter()
            .map(|entry| &entry.track)
            .chain(liked.iter().map(|(track, _)| track))
        {
            artist_names
                .entry(artist_id(track))
                .or_insert_with(|| track.display_artist());
        }
        let play_events = history
            .iter()
            .map(|entry| (track_key(&entry.track), entry.played_at_ms))
            .collect();
        Self {
            recent_tracks,
            recent_track_keys,
            recent_artist_ids,
            recent_top_artists,
            all_top_artists,
            liked_top_artists_recent,
            liked_top_artists_alltime,
            recent_artist_map,
            all_artist_map,
            liked_track_keys,
            liked_artist_ids,
            artist_names,
            play_events,
        }
    }

    pub fn play_counts_since(&self, since_ms: i64) -> HashMap<String, i64> {
        let mut result = HashMap::new();
        for (key, played_at_ms) in &self.play_events {
            if since_ms > 0 && *played_at_ms < since_ms {
                continue;
            }
            result
                .entry(key.clone())
                .and_modify(|count| *count += 1)
                .or_insert(1);
        }
        result
    }

    pub fn cooldown_keys(&self, now_ms: i64, cooldown_hours: i64) -> HashSet<String> {
        let threshold = now_ms - cooldown_hours.max(1) * 3_600_000;
        self.play_events
            .iter()
            .filter(|(_, played_at_ms)| *played_at_ms >= threshold)
            .map(|(key, _)| key.clone())
            .collect()
    }
}

fn recent_unique_tracks(history: &[HistoryEntry], limit: usize) -> Vec<TrackRef> {
    let mut seen = HashSet::new();
    history
        .iter()
        .filter_map(|entry| {
            if seen.insert(track_key(&entry.track)) {
                Some(entry.track.clone())
            } else {
                None
            }
        })
        .take(limit)
        .collect()
}

fn recent_unique_artists(history: &[HistoryEntry], limit: usize) -> Vec<String> {
    let mut seen = HashSet::new();
    history
        .iter()
        .filter_map(|entry| {
            let id = artist_id(&entry.track);
            if !id.is_empty() && seen.insert(id.clone()) {
                Some(id)
            } else {
                None
            }
        })
        .take(limit)
        .collect()
}

fn rank_history_artists(
    history: &[HistoryEntry],
    since_ms: i64,
    limit: usize,
) -> Vec<(String, i64)> {
    rank_artists(
        history
            .iter()
            .filter(|entry| since_ms <= 0 || entry.played_at_ms >= since_ms)
            .map(|entry| (artist_id(&entry.track), entry.played_at_ms)),
        limit,
    )
}

fn rank_liked_artists(
    liked: &[(TrackRef, i64)],
    since_ms: i64,
    limit: usize,
) -> Vec<(String, i64)> {
    rank_artists(
        liked
            .iter()
            .filter(|(_, liked_at_ms)| since_ms <= 0 || *liked_at_ms >= since_ms)
            .map(|(track, liked_at_ms)| (artist_id(track), *liked_at_ms)),
        limit,
    )
}

fn rank_artists(events: impl Iterator<Item = (String, i64)>, limit: usize) -> Vec<(String, i64)> {
    let mut counted = HashMap::<String, (i64, i64)>::new();
    for (artist, timestamp) in events {
        if artist.is_empty() {
            continue;
        }
        counted
            .entry(artist)
            .and_modify(|entry| {
                entry.0 += 1;
                entry.1 = entry.1.max(timestamp);
            })
            .or_insert((1, timestamp));
    }
    let mut ranked = counted.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .1
            .0
            .cmp(&left.1.0)
            .then_with(|| right.1.1.cmp(&left.1.1))
            .then_with(|| left.0.cmp(&right.0))
    });
    ranked
        .into_iter()
        .take(limit)
        .map(|(artist, (count, _))| (artist, count))
        .collect()
}
