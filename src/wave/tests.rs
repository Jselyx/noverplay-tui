use crate::model::ProviderKind;

use super::{WaveMode, WaveMood, WaveQueueQuotas, WaveSettings, WaveSourceMode};

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
