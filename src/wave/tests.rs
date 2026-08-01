use std::collections::HashSet;

use crate::{
    model::{PlaybackCapability, ProviderKind, TrackRef},
    storage::HistoryEntry,
};
use url::Url;

use super::{
    WaveCandidate, WaveCandidateOrigin, WaveGenreProfile, WaveMode, WaveMood, WaveQueueQuotas,
    WaveRankInput, WaveReason, WaveSettings, WaveSourceMode, WaveTasteProfile, WaveTimeOfDay,
    rank_candidates, select_ranked,
};

#[test]
fn pc_mode_aliases_stay_compatible() {
    assert_eq!(WaveMode::normalize(Some("recent")), WaveMode::Balanced);
    assert_eq!(WaveMode::normalize(Some("explore")), WaveMode::Discovery);
    assert_eq!(WaveMode::normalize(Some("likes")), WaveMode::Favorites);
    assert_eq!(WaveMode::normalize(Some("related")), WaveMode::Radio);
    assert_eq!(WaveMood::normalize(Some("workout")), WaveMood::Drive);
    assert_eq!(
        WaveSourceMode::normalize(Some("mix")),
        WaveSourceMode::FallbackSoft
    );
    assert_eq!(
        WaveTimeOfDay::normalize(Some("evening")),
        WaveTimeOfDay::Evening
    );
}

#[test]
fn pc_balanced_quotas_keep_same_ratio() {
    let quotas = WaveQueueQuotas::calculate(20, WaveMode::Balanced, 0.35);
    assert_eq!(quotas.total(), 20);
    assert_eq!(quotas.core, 3);
    assert_eq!(quotas.related, 8);
    assert_eq!(quotas.favorites, 5);
    assert_eq!(quotas.discovery, 4);
}

#[test]
fn every_quota_mode_closes_exactly() {
    for mode in [
        WaveMode::Balanced,
        WaveMode::Discovery,
        WaveMode::Favorites,
        WaveMode::Radio,
    ] {
        for size in 1..=80 {
            assert_eq!(WaveQueueQuotas::calculate(size, mode, 0.8).total(), size);
        }
    }
}

#[test]
fn wave_settings_clamp_like_pc_and_skip_deezer() {
    let settings = WaveSettings {
        primary_provider: ProviderKind::Deezer,
        size: 999,
        anti_repeat_hours: 0,
        max_plays: 999,
        play_window_days: 0,
        novelty: 5.0,
        max_artist_streak: 99,
        language_rotation: vec![
            "rus".to_string(),
            "RU".to_string(),
            "english".to_string(),
            "xx".to_string(),
        ],
        source_mode: WaveSourceMode::FallbackSoft,
        ..WaveSettings::default()
    }
    .normalized(false);
    assert_eq!(settings.size, 80);
    assert_eq!(settings.anti_repeat_hours, 1);
    assert_eq!(settings.max_plays, 20);
    assert_eq!(settings.play_window_days, 1);
    assert_eq!(settings.novelty, 1.0);
    assert_eq!(settings.max_artist_streak, 4);
    assert_eq!(settings.language_rotation, ["ru", "en"]);
    assert_eq!(
        settings.provider_order(),
        [ProviderKind::YandexMusic, ProviderKind::SoundCloud]
    );
}

#[test]
fn profile_uses_pc_windows_and_unique_recent_tracks() {
    let now = 2_000_000_000_000_i64;
    let history = vec![
        history(track("1", "Artist A"), now - 1_000),
        history(track("1", "Artist A"), now - 2_000),
        history(track("2", "Artist B"), now - 15 * 86_400_000),
    ];
    let liked = vec![
        (track("3", "Artist A"), now - 1_000),
        (track("4", "Artist C"), now - 31 * 86_400_000),
    ];
    let profile = WaveTasteProfile::build(&history, &liked, now);
    assert_eq!(profile.recent_tracks.len(), 2);
    assert_eq!(profile.recent_top_artists, [("artist a".to_string(), 2)]);
    assert_eq!(
        profile.all_top_artists,
        [("artist a".to_string(), 2), ("artist b".to_string(), 1)]
    );
    assert_eq!(
        profile.liked_top_artists_recent,
        [("artist a".to_string(), 1)]
    );
    assert!(profile.liked_artist_ids.contains("artist c"));
}

#[test]
fn profile_counts_repeats_and_cooldown_like_pc() {
    let now = 2_000_000_000_000_i64;
    let history = vec![
        history(track("1", "Artist A"), now - 30 * 60_000),
        history(track("1", "Artist A"), now - 25 * 3_600_000),
        history(track("2", "Artist B"), now - 2 * 3_600_000),
    ];
    let profile = WaveTasteProfile::build(&history, &[], now);
    assert_eq!(profile.play_counts_since(now - 24 * 3_600_000).len(), 2);
    assert_eq!(profile.play_counts_since(0).values().sum::<i64>(), 3);
    let cooldown = profile.cooldown_keys(now, 24);
    assert!(cooldown.contains("SoundCloud:1"));
    assert!(cooldown.contains("SoundCloud:2"));
}

#[test]
fn candidate_filter_throws_video_and_podcast_in_the_bin() {
    let mut video = track("video", "Artist");
    video.title = "Song Official Video".to_string();
    assert!(!WaveCandidate::new(video, WaveCandidateOrigin::Seed).is_tracklike());
    let mut podcast = track("podcast", "Podcast Author");
    podcast.title = "Episode 10".to_string();
    assert!(!WaveCandidate::new(podcast, WaveCandidateOrigin::Seed).is_tracklike());
    assert!(WaveCandidate::new(track("song", "Artist"), WaveCandidateOrigin::Seed).is_tracklike());
}

#[test]
fn candidate_bucket_keeps_pc_precedence() {
    let mut candidate = WaveCandidate::new(track("1", "Artist"), WaveCandidateOrigin::Related);
    candidate.add_origin(WaveCandidateOrigin::Explore);
    candidate.add_origin(WaveCandidateOrigin::Comfort);
    assert_eq!(candidate.bucket(), super::WaveBucket::Favorites);
}

#[test]
fn genre_similarity_uses_the_same_weighted_pc_tags() {
    let mut rock = track("rock", "Artist");
    rock.title = "Night Rock".to_string();
    let mut jazz = track("jazz", "Artist");
    jazz.title = "Quiet Jazz".to_string();
    let profile = WaveGenreProfile::from_tracks(&[
        rock.clone(),
        rock.clone(),
        rock.clone(),
        rock.clone(),
        jazz.clone(),
    ]);
    assert!(profile.similarity(&rock) > profile.similarity(&jazz));
    assert_eq!(profile.similarity(&track("plain", "Artist")), 0.0);
}

#[test]
fn ranking_filters_cooldown_and_exact_likes_like_pc() {
    let now = 2_000_000_000_000_i64;
    let recent = track("recent", "Known");
    let liked = track("liked", "Liked");
    let fresh = track("fresh", "Fresh");
    let history = vec![history(recent.clone(), now - 1_000)];
    let likes = vec![(liked.clone(), now - 1_000)];
    let profile = WaveTasteProfile::build(&history, &likes, now);
    let genre = WaveGenreProfile::from_tracks(&profile.recent_tracks);
    let settings = WaveSettings {
        primary_provider: ProviderKind::SoundCloud,
        ..WaveSettings::default()
    };
    let ranked = rank_candidates(WaveRankInput {
        candidates: vec![recent, liked, fresh.clone()]
            .into_iter()
            .map(|track| WaveCandidate::new(track, WaveCandidateOrigin::Seed))
            .collect(),
        profile: &profile,
        genre_profile: &genre,
        settings: &settings,
        now_ms: now,
        exclude_track_keys: &HashSet::new(),
        exclude_artist_ids: &HashSet::new(),
        strict_seed_artist_ids: &HashSet::new(),
    });
    assert_eq!(ranked.len(), 1);
    assert_eq!(ranked[0].track, fresh);
}

#[test]
fn balanced_score_prefers_recent_artist_with_pc_weights() {
    let now = 2_000_000_000_000_i64;
    let history = vec![history(track("old", "Known"), now - 25 * 3_600_000)];
    let profile = WaveTasteProfile::build(&history, &[], now);
    let genre = WaveGenreProfile::default();
    let settings = WaveSettings {
        primary_provider: ProviderKind::SoundCloud,
        ..WaveSettings::default()
    };
    let known = track("known-next", "Known");
    let unknown = track("unknown", "Unknown");
    let ranked = rank_candidates(WaveRankInput {
        candidates: vec![known.clone(), unknown]
            .into_iter()
            .map(|track| WaveCandidate::new(track, WaveCandidateOrigin::Seed))
            .collect(),
        profile: &profile,
        genre_profile: &genre,
        settings: &settings,
        now_ms: now,
        exclude_track_keys: &HashSet::new(),
        exclude_artist_ids: &HashSet::new(),
        strict_seed_artist_ids: &HashSet::new(),
    });
    assert_eq!(ranked[0].track, known);
}

#[test]
fn selector_enforces_artist_streak_without_shrinking_queue() {
    let settings = WaveSettings {
        size: 3,
        max_artist_streak: 1,
        primary_provider: ProviderKind::SoundCloud,
        ..WaveSettings::default()
    };
    let ranked = vec![
        ranked("1", "Same", 3.0),
        ranked("2", "Same", 2.0),
        ranked("3", "Other", 1.0),
    ];
    let selected = select_ranked(
        ranked,
        &settings,
        WaveQueueQuotas {
            core: 3,
            related: 0,
            favorites: 0,
            discovery: 0,
        },
    );
    assert_eq!(selected.len(), 3);
    assert_eq!(selected[0].track.id, "1");
    assert_eq!(selected[1].track.id, "3");
    assert_eq!(selected[2].track.id, "2");
}

#[test]
fn selector_rotates_language_then_falls_back_like_pc() {
    let settings = WaveSettings {
        size: 2,
        primary_provider: ProviderKind::SoundCloud,
        language_rotation: vec!["ru".to_string(), "en".to_string()],
        ..WaveSettings::default()
    };
    let mut english = ranked("en", "Artist", 5.0);
    english.track.title = "English Track".to_string();
    let mut russian = ranked("ru", "Исполнитель", 1.0);
    russian.track.title = "Русский трек".to_string();
    let selected = select_ranked(
        vec![english, russian],
        &settings,
        WaveQueueQuotas {
            core: 2,
            related: 0,
            favorites: 0,
            discovery: 0,
        },
    );
    assert_eq!(selected[0].track.id, "ru");
    assert_eq!(selected[1].track.id, "en");
}

#[test]
fn explore_queries_keep_pc_order_and_novelty_limit() {
    let settings = WaveSettings {
        mode: WaveMode::Discovery,
        mood: WaveMood::Drive,
        time_of_day: WaveTimeOfDay::Night,
        novelty: 0.8,
        language_rotation: vec!["en".to_string()],
        ..WaveSettings::default()
    };
    let queries = super::explore_queries(&[], &settings);
    assert_eq!(queries.len(), 7);
    assert_eq!(queries[0], "new tracks");
    assert!(queries.contains(&"high energy songs".to_string()));
}

#[test]
fn title_keywords_drop_pc_stop_words_and_duplicates() {
    let mut first = track("one", "Artist");
    first.title = "Official Winter Winter Remix".to_string();
    let mut second = track("two", "Artist");
    second.title = "Night Winter".to_string();
    assert_eq!(
        super::recent_title_keywords(&[first, second], 5),
        ["winter", "night"]
    );
}

fn history(track: TrackRef, played_at_ms: i64) -> HistoryEntry {
    HistoryEntry {
        track,
        played_at_ms,
        completed: true,
        skipped: false,
    }
}

fn track(id: &str, artist: &str) -> TrackRef {
    TrackRef {
        provider: ProviderKind::SoundCloud,
        id: id.to_string(),
        title: format!("Track {id}"),
        artists: vec![artist.to_string()],
        duration_ms: Some(180_000),
        artwork_url: None,
        web_url: Url::parse(&format!("https://soundcloud.com/artist/{id}")).unwrap(),
        capability: PlaybackCapability::Full,
        genres: Vec::new(),
        explicit: false,
        drm: false,
    }
}

fn ranked(id: &str, artist: &str, score: f64) -> super::RankedWaveTrack {
    let track = track(id, artist);
    super::RankedWaveTrack {
        key: track.provider_key(),
        artist_id: super::artist_id(&track),
        track,
        is_new_artist: false,
        is_liked_exact: false,
        bucket: super::WaveBucket::Core,
        score,
        reason: WaveReason::TasteMatch,
    }
}
