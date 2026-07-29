use std::collections::HashSet;

use crate::model::TrackRef;

use super::{WaveMode, WaveMood, WaveSettings, WaveTimeOfDay, text::normalize_search_text};

pub fn explore_queries(recent: &[TrackRef], settings: &WaveSettings) -> Vec<String> {
    let wants_ru = settings.language_rotation.is_empty()
        || settings
            .language_rotation
            .iter()
            .any(|language| language == "ru");
    let wants_en = settings.language_rotation.is_empty()
        || settings
            .language_rotation
            .iter()
            .any(|language| language == "en");
    let mut queries = UniqueQueries::default();
    if wants_ru {
        queries.push(match settings.mode {
            WaveMode::Discovery => "новинки музыки",
            WaveMode::Radio => "радио по артисту",
            WaveMode::Favorites => "похожие на любимое",
            WaveMode::Balanced => "музыка по вкусу",
        });
        for query in [
            "популярные треки",
            "инди",
            "драйв",
            "танцевальная музыка",
            "электроника",
        ] {
            queries.push(query);
        }
    }
    if wants_en {
        queries.push(match settings.mode {
            WaveMode::Discovery => "new tracks",
            WaveMode::Radio => "artist radio",
            WaveMode::Favorites => "similar to liked songs",
            WaveMode::Balanced => "music by taste",
        });
        for query in [
            "trending songs",
            "indie",
            "dance hits",
            "electronic",
            "night drive",
        ] {
            queries.push(query);
        }
    }
    add_mood_queries(&mut queries, settings.mood, wants_ru, wants_en);
    add_time_queries(&mut queries, settings.time_of_day, wants_ru, wants_en);
    add_corpus_queries(&mut queries, recent, wants_ru, wants_en);
    for keyword in recent_title_keywords(recent, 8) {
        if wants_ru {
            queries.push(format!("{keyword} микс"));
        }
        if wants_en {
            queries.push(format!("{keyword} mix"));
        }
    }
    let limit = if settings.novelty >= 0.9 {
        8
    } else if settings.novelty >= 0.7 {
        7
    } else if settings.novelty >= 0.45 {
        6
    } else {
        5
    };
    queries.values.truncate(limit);
    queries.values
}

pub fn recent_title_keywords(recent: &[TrackRef], limit: usize) -> Vec<String> {
    let stop_words = [
        "feat", "ft", "official", "video", "mix", "version", "radio", "edit", "live", "remix",
        "track", "song", "music", "and", "the", "for", "with", "на", "для", "это", "feat", "prod",
        "club", "unknown", "artist", "songs", "best",
    ]
    .into_iter()
    .collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for track in recent {
        for token in normalize_search_text(&track.title).split_whitespace() {
            if token.len() < 4
                || stop_words.contains(token)
                || token.chars().all(|symbol| symbol.is_ascii_digit())
                || !seen.insert(token.to_string())
            {
                continue;
            }
            result.push(token.to_string());
            if result.len() >= limit {
                return result;
            }
        }
    }
    result
}

#[derive(Default)]
struct UniqueQueries {
    values: Vec<String>,
    seen: HashSet<String>,
}

impl UniqueQueries {
    fn push(&mut self, value: impl Into<String>) {
        let value = value.into().trim().to_string();
        if !value.is_empty() && self.seen.insert(value.clone()) {
            self.values.push(value);
        }
    }
}

fn add_mood_queries(queries: &mut UniqueQueries, mood: WaveMood, ru: bool, en: bool) {
    let (russian, english): (&[&str], &[&str]) = match mood {
        WaveMood::Calm => (&["спокойная музыка", "чилл"], &["calm music", "chill mix"]),
        WaveMood::Drive => (
            &["энергичная музыка", "музыка для тренировки"],
            &["high energy songs", "workout mix"],
        ),
        WaveMood::Focus => (
            &["музыка для концентрации", "лоуфай для работы"],
            &["focus music", "lofi for work"],
        ),
        WaveMood::Night => (
            &["ночной вайб", "ночная музыка"],
            &["night drive", "late night songs"],
        ),
        WaveMood::Auto => (&[], &[]),
    };
    if ru {
        for query in russian {
            queries.push(*query);
        }
    }
    if en {
        for query in english {
            queries.push(*query);
        }
    }
}

fn add_time_queries(queries: &mut UniqueQueries, time: WaveTimeOfDay, ru: bool, en: bool) {
    let pair = match time {
        WaveTimeOfDay::Morning => Some(("утренний плейлист", "morning playlist")),
        WaveTimeOfDay::Day => Some(("дневной плейлист", "daytime vibes")),
        WaveTimeOfDay::Evening => Some(("вечерний плейлист", "evening chill")),
        WaveTimeOfDay::Night => Some(("ночной вайб", "night drive")),
        WaveTimeOfDay::Auto => None,
    };
    if let Some((russian, english)) = pair {
        if ru {
            queries.push(russian);
        }
        if en {
            queries.push(english);
        }
    }
}

fn add_corpus_queries(queries: &mut UniqueQueries, recent: &[TrackRef], ru: bool, en: bool) {
    let corpus = normalize_search_text(
        &recent
            .iter()
            .flat_map(|track| [track.title.clone(), track.display_artist()])
            .collect::<Vec<_>>()
            .join(" "),
    );
    let groups: &[(&[&str], &str, &str)] = &[
        (
            &["груст", "печал", "sad", "cry"],
            "грустные треки",
            "sad songs",
        ),
        (
            &["любов", "роман", "love", "heart"],
            "романтика",
            "romantic",
        ),
        (
            &["танц", "club", "dance", "house"],
            "клубная музыка",
            "club mix",
        ),
        (&["рок", "rock", "metal"], "рок", "rock"),
        (&["рэп", "rap", "hiphop", "hip-hop"], "рэп", "rap"),
        (&["фонк", "phonk"], "фонк", "phonk"),
    ];
    for (markers, russian, english) in groups {
        if !markers.iter().any(|marker| corpus.contains(marker)) {
            continue;
        }
        if ru {
            queries.push(*russian);
        }
        if en {
            queries.push(*english);
        }
    }
}
