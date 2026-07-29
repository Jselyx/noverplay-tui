use super::{RankedWaveTrack, WaveBucket, WaveMode, WaveQueueQuotas, WaveSettings, WaveSourceMode};

pub fn select_ranked(
    ranked: Vec<RankedWaveTrack>,
    settings: &WaveSettings,
    quotas: WaveQueueQuotas,
) -> Vec<RankedWaveTrack> {
    let mut state = SelectionState {
        final_tracks: Vec::new(),
        pool: ranked,
        picked_new_tracks: 0,
        liked_exact_picked: 0,
        language_cursor: 0,
    };
    let liked_exact_cap = if settings.mode == WaveMode::Favorites
        || settings.source_mode == WaveSourceMode::LibraryOnly
    {
        ((settings.size as f64) * 0.25).ceil() as usize
    } else {
        0
    };
    let rules = SelectionRules {
        queue_size: settings.size,
        max_artist_streak: settings.max_artist_streak,
        target_new_tracks: quotas.discovery,
        liked_exact_cap,
        languages: &settings.language_rotation,
    };
    for (bucket, count) in [
        (Some(WaveBucket::Favorites), quotas.favorites),
        (Some(WaveBucket::Core), quotas.core),
        (Some(WaveBucket::Related), quotas.related),
        (Some(WaveBucket::Discovery), quotas.discovery),
        (None, settings.size),
    ] {
        pick_from_pool(bucket, count, &mut state, &rules);
    }
    state.final_tracks
}

struct SelectionState {
    final_tracks: Vec<RankedWaveTrack>,
    pool: Vec<RankedWaveTrack>,
    picked_new_tracks: usize,
    liked_exact_picked: usize,
    language_cursor: usize,
}

struct SelectionRules<'a> {
    queue_size: usize,
    max_artist_streak: usize,
    target_new_tracks: usize,
    liked_exact_cap: usize,
    languages: &'a [String],
}

fn pick_from_pool(
    target_bucket: Option<WaveBucket>,
    target_count: usize,
    state: &mut SelectionState,
    rules: &SelectionRules<'_>,
) {
    let mut picked_for_bucket = 0;
    while state.final_tracks.len() < rules.queue_size
        && picked_for_bucket < target_count
        && !state.pool.is_empty()
    {
        let preferred_language = if rules.languages.is_empty() {
            None
        } else {
            Some(rules.languages[state.language_cursor % rules.languages.len()].as_str())
        };
        let mut pick_index = None;
        for require_language in [true, false] {
            if pick_index.is_some() {
                break;
            }
            for (index, item) in state.pool.iter().enumerate() {
                if target_bucket.is_some_and(|bucket| item.bucket != bucket)
                    || !artist_streak_allowed(&state.final_tracks, item, rules.max_artist_streak)
                    || item.is_liked_exact
                        && (state.liked_exact_picked >= rules.liked_exact_cap
                            || state
                                .final_tracks
                                .last()
                                .is_some_and(|track| track.is_liked_exact))
                    || target_bucket == Some(WaveBucket::Discovery)
                        && state.picked_new_tracks.saturating_add(1) <= rules.target_new_tracks
                        && !item.is_new_artist
                    || require_language
                        && preferred_language.is_some_and(|language| {
                            detect_language(&item.track.title, &item.track.display_artist())
                                != language
                        })
                {
                    continue;
                }
                pick_index = Some(index);
                break;
            }
        }
        let Some(index) = pick_index else {
            break;
        };
        let picked = state.pool.remove(index);
        if picked.is_new_artist {
            state.picked_new_tracks = state.picked_new_tracks.saturating_add(1);
        }
        if picked.is_liked_exact {
            state.liked_exact_picked = state.liked_exact_picked.saturating_add(1);
        }
        state.final_tracks.push(picked);
        picked_for_bucket = picked_for_bucket.saturating_add(1);
        if !rules.languages.is_empty() {
            state.language_cursor = state.language_cursor.saturating_add(1);
        }
    }
}

fn artist_streak_allowed(
    current: &[RankedWaveTrack],
    item: &RankedWaveTrack,
    max_artist_streak: usize,
) -> bool {
    let Some(last_artist) = current.last().map(|track| track.artist_id.as_str()) else {
        return true;
    };
    if last_artist != item.artist_id {
        return true;
    }
    current
        .iter()
        .rev()
        .take_while(|track| track.artist_id == last_artist)
        .count()
        < max_artist_streak
}

fn detect_language(title: &str, artist: &str) -> &'static str {
    let mut cyrillic = 0;
    let mut latin = 0;
    for symbol in title.chars().chain(artist.chars()) {
        if symbol.is_ascii_alphabetic() {
            latin += 1;
        } else if ('\u{0400}'..='\u{04ff}').contains(&symbol)
            || ('\u{0500}'..='\u{052f}').contains(&symbol)
        {
            cyrillic += 1;
        }
    }
    if cyrillic == 0 && latin == 0 {
        "other"
    } else if cyrillic >= latin && cyrillic > 0 {
        "ru"
    } else {
        "en"
    }
}
