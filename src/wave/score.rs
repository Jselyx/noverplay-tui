use std::{cmp::Ordering, collections::HashSet};

use crate::model::TrackRef;

use super::{
    WaveBucket, WaveCandidate, WaveGenreProfile, WaveMode, WaveSettings, WaveSourceMode,
    WaveTasteProfile, artist_id, track_key,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaveReason {
    Liked,
    NewDiscovery,
    RecentArtist,
    FavoriteArtist,
    GenreMatch,
    TasteMatch,
}

#[derive(Clone, Debug)]
pub struct RankedWaveTrack {
    pub track: TrackRef,
    pub key: String,
    pub artist_id: String,
    pub is_new_artist: bool,
    pub is_liked_exact: bool,
    pub bucket: WaveBucket,
    pub score: f64,
    pub reason: WaveReason,
}

pub struct WaveRankInput<'a> {
    pub candidates: Vec<WaveCandidate>,
    pub profile: &'a WaveTasteProfile,
    pub genre_profile: &'a WaveGenreProfile,
    pub settings: &'a WaveSettings,
    pub now_ms: i64,
    pub exclude_track_keys: &'a HashSet<String>,
    pub exclude_artist_ids: &'a HashSet<String>,
    pub strict_seed_artist_ids: &'a HashSet<String>,
}

pub fn rank_candidates(input: WaveRankInput<'_>) -> Vec<RankedWaveTrack> {
    let settings = input.settings;
    let profile = input.profile;
    let provider_order = settings
        .provider_order()
        .into_iter()
        .collect::<HashSet<_>>();
    let cooldown_keys = profile.cooldown_keys(input.now_ms, settings.anti_repeat_hours);
    let window_play_counts = profile
        .play_counts_since(input.now_ms - settings.play_window_days.saturating_mul(86_400_000));
    let all_play_counts = profile.play_counts_since(0);
    let known_artist_ids = profile.all_artist_map.keys().collect::<HashSet<_>>();
    let max_recent_artist_weight = profile
        .recent_top_artists
        .first()
        .map(|(_, count)| *count as f64)
        .unwrap_or(1.0)
        .max(1.0);
    let max_all_artist_weight = profile
        .all_top_artists
        .first()
        .map(|(_, count)| *count as f64)
        .unwrap_or(1.0)
        .max(1.0);
    let strict_artist_mode = settings.novelty <= 0.001 && !input.strict_seed_artist_ids.is_empty();
    let allow_liked_exact =
        settings.mode == WaveMode::Favorites || settings.source_mode == WaveSourceMode::LibraryOnly;
    let mut ranked = Vec::new();
    for candidate in input.candidates {
        let track = candidate.track.clone();
        let key = track_key(&track);
        let candidate_artist_id = artist_id(&track);
        if key.is_empty()
            || !provider_order.contains(&track.provider)
            || !candidate.is_tracklike()
            || input.exclude_track_keys.contains(&key)
            || input.exclude_artist_ids.contains(&candidate_artist_id)
            || strict_artist_mode && !input.strict_seed_artist_ids.contains(&candidate_artist_id)
            || cooldown_keys.contains(&key)
            || window_play_counts.get(&key).copied().unwrap_or_default() >= settings.max_plays
        {
            continue;
        }
        let is_liked_exact = profile.liked_track_keys.contains(&key);
        if is_liked_exact && !allow_liked_exact {
            continue;
        }
        let artist_recent = profile
            .recent_artist_map
            .get(&candidate_artist_id)
            .copied()
            .unwrap_or_default() as f64
            / max_recent_artist_weight;
        let artist_all = profile
            .all_artist_map
            .get(&candidate_artist_id)
            .copied()
            .unwrap_or_default() as f64
            / max_all_artist_weight;
        let is_new_artist = !known_artist_ids.contains(&candidate_artist_id);
        let liked_boost = liked_boost(
            settings.mode,
            is_liked_exact,
            profile.liked_artist_ids.contains(&candidate_artist_id),
        );
        let played_total = all_play_counts.get(&key).copied().unwrap_or_default() as f64;
        let bucket = candidate.bucket();
        let repeat_penalty = (1.0 - settings.novelty) * (played_total / 4.0).min(2.8);
        let novelty_bonus = if played_total <= 0.0 {
            1.15 * settings.novelty
        } else {
            0.18 * settings.novelty
        };
        let new_artist_bonus = if !strict_artist_mode && is_new_artist {
            1.6 * settings.novelty
        } else {
            0.0
        };
        let familiar_artist_penalty = if is_new_artist {
            0.0
        } else {
            settings.novelty * (0.95 + artist_all * 1.15 + artist_recent * 0.6)
        };
        let similarity_bonus = if profile.recent_artist_ids.contains(&candidate_artist_id) {
            0.85
        } else {
            0.25
        };
        let genre_similarity = input.genre_profile.similarity(&track);
        let genre_bonus = if matches!(settings.mode, WaveMode::Balanced | WaveMode::Radio) {
            1.55 * genre_similarity
        } else {
            1.15 * genre_similarity
        };
        let bucket_bonus = bucket_bonus(settings.mode, bucket);
        let service_bonus = if track.provider == settings.primary_provider {
            0.45
        } else {
            -0.35
        };
        let score = calculate_score(ScoreParts {
            mode: settings.mode,
            artist_recent,
            artist_all,
            liked_boost,
            similarity_bonus,
            genre_bonus,
            novelty_bonus,
            new_artist_bonus,
            bucket_bonus,
            service_bonus,
            familiar_artist_penalty,
            repeat_penalty,
        });
        let reason = select_reason(
            is_liked_exact,
            played_total,
            is_new_artist,
            artist_recent,
            artist_all,
            genre_similarity,
        );
        ranked.push(RankedWaveTrack {
            track,
            key,
            artist_id: candidate_artist_id,
            is_new_artist,
            is_liked_exact,
            bucket,
            score,
            reason,
        });
    }
    ranked.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.key.cmp(&right.key))
    });
    ranked
}

fn liked_boost(mode: WaveMode, exact: bool, artist: bool) -> f64 {
    if exact {
        if mode == WaveMode::Favorites {
            0.55
        } else {
            0.35
        }
    } else if artist {
        if mode == WaveMode::Favorites {
            0.35
        } else {
            0.25
        }
    } else {
        0.0
    }
}

fn bucket_bonus(mode: WaveMode, bucket: WaveBucket) -> f64 {
    match (mode, bucket) {
        (WaveMode::Favorites, WaveBucket::Favorites) => 1.45,
        (WaveMode::Radio, WaveBucket::Related) => 1.35,
        (WaveMode::Discovery, WaveBucket::Discovery) => 1.25,
        (WaveMode::Balanced, WaveBucket::Core) => 0.65,
        (WaveMode::Balanced, WaveBucket::Related) => 0.70,
        (WaveMode::Balanced, WaveBucket::Discovery) => 0.45,
        _ => 0.0,
    }
}

struct ScoreParts {
    mode: WaveMode,
    artist_recent: f64,
    artist_all: f64,
    liked_boost: f64,
    similarity_bonus: f64,
    genre_bonus: f64,
    novelty_bonus: f64,
    new_artist_bonus: f64,
    bucket_bonus: f64,
    service_bonus: f64,
    familiar_artist_penalty: f64,
    repeat_penalty: f64,
}

fn calculate_score(parts: ScoreParts) -> f64 {
    match parts.mode {
        WaveMode::Balanced => {
            1.8 * parts.artist_recent
                + 0.85 * parts.artist_all
                + 0.9 * parts.liked_boost
                + parts.similarity_bonus
                + parts.genre_bonus
                + parts.novelty_bonus
                + parts.new_artist_bonus
                + parts.bucket_bonus
                + parts.service_bonus
                - parts.familiar_artist_penalty
                - parts.repeat_penalty
        }
        WaveMode::Discovery => {
            0.65 * parts.artist_recent
                + 0.55 * parts.artist_all
                + 0.55 * parts.liked_boost
                + 0.85 * parts.genre_bonus
                + 1.9 * parts.novelty_bonus
                + 1.8 * parts.new_artist_bonus
                + parts.bucket_bonus
                + parts.service_bonus
                - parts.repeat_penalty * 0.85
        }
        WaveMode::Favorites => {
            0.8 * parts.artist_recent
                + 1.55 * parts.artist_all
                + 1.35 * parts.liked_boost
                + 0.75 * parts.similarity_bonus
                + 0.85 * parts.genre_bonus
                + parts.bucket_bonus
                + parts.service_bonus
                - parts.repeat_penalty
        }
        WaveMode::Radio => {
            1.1 * parts.artist_recent
                + 1.1 * parts.artist_all
                + 0.75 * parts.liked_boost
                + 1.15 * parts.similarity_bonus
                + 1.3 * parts.genre_bonus
                + parts.novelty_bonus
                + parts.new_artist_bonus
                + parts.bucket_bonus
                + parts.service_bonus
                - parts.familiar_artist_penalty
                - parts.repeat_penalty
        }
    }
}

fn select_reason(
    liked: bool,
    played_total: f64,
    new_artist: bool,
    artist_recent: f64,
    artist_all: f64,
    genre_similarity: f64,
) -> WaveReason {
    if liked {
        WaveReason::Liked
    } else if played_total <= 0.0 || new_artist {
        WaveReason::NewDiscovery
    } else if artist_recent > 0.5 {
        WaveReason::RecentArtist
    } else if artist_all > 0.55 {
        WaveReason::FavoriteArtist
    } else if genre_similarity >= 0.28 {
        WaveReason::GenreMatch
    } else {
        WaveReason::TasteMatch
    }
}
