use std::collections::{HashMap, HashSet};

use crate::model::TrackRef;

use super::text::normalize_search_text;

#[derive(Clone, Debug, Default)]
pub struct WaveGenreProfile {
    weights: HashMap<String, usize>,
}

impl WaveGenreProfile {
    pub fn from_tracks(tracks: &[TrackRef]) -> Self {
        let mut weights = HashMap::new();
        for track in tracks {
            for tag in track_tags(track) {
                weights
                    .entry(tag)
                    .and_modify(|count| *count += 1)
                    .or_insert(1);
            }
        }
        Self { weights }
    }

    pub fn similarity(&self, track: &TrackRef) -> f64 {
        if self.weights.is_empty() {
            return 0.0;
        }
        let tags = track_tags(track);
        if tags.is_empty() {
            return 0.0;
        }
        let total_weight = self.weights.values().copied().sum::<usize>().max(1) as f64;
        let matched_weight = tags
            .iter()
            .map(|tag| self.weights.get(tag).copied().unwrap_or_default() as f64)
            .sum::<f64>();
        (matched_weight / total_weight * 4.0).clamp(0.0, 1.0)
    }
}

fn track_tags(track: &TrackRef) -> HashSet<String> {
    extract_tags(&format!(" {} {} ", track.title, track.display_artist()))
}

fn extract_tags(value: &str) -> HashSet<String> {
    let source = format!(" {} ", normalize_search_text(value));
    let mut tags = HashSet::new();
    let mut insert_if_any = |tag: &str, keys: &[&str]| {
        if keys.iter().any(|key| source.contains(key)) {
            tags.insert(tag.to_string());
        }
    };
    insert_if_any(
        "rap",
        &[
            " rap ",
            " рэп",
            " hip hop",
            " hip-hop",
            " хип хоп",
            " хип-хоп",
        ],
    );
    insert_if_any("rock", &[" рок", " rock", " grunge"]);
    insert_if_any("metal", &[" metal", " метал"]);
    insert_if_any("pop", &[" pop", " поп"]);
    insert_if_any(
        "electronic",
        &[" electro", " электро", " edm", " electronic"],
    );
    insert_if_any("house", &[" house", " хаус"]);
    insert_if_any("techno", &[" techno", " техно"]);
    insert_if_any("phonk", &[" phonk", " фонк"]);
    insert_if_any("lofi", &[" lofi", " lo-fi", " лоуфай", " лофи"]);
    insert_if_any("ambient", &[" ambient", " эмбиент"]);
    insert_if_any("jazz", &[" jazz", " джаз"]);
    insert_if_any("classical", &[" classical", " классика", " orches"]);
    insert_if_any("indie", &[" indie", " инди"]);
    insert_if_any("punk", &[" punk", " панк"]);
    insert_if_any("trap", &[" trap", " трэп", " треп"]);
    insert_if_any("dnb", &[" dnb", " drum and bass", " драм"]);
    insert_if_any("dubstep", &[" dubstep", " дабстеп"]);
    insert_if_any("rnb", &[" rnb", " r&b"]);
    insert_if_any("soul", &[" soul", " соул"]);
    insert_if_any("funk", &[" funk", " фанк"]);
    insert_if_any("chill", &[" chill", " чилл", " relaxed", " спокой"]);
    insert_if_any("sad", &[" sad", " груст", " печал"]);
    insert_if_any("dance", &[" dance", " танц", " club", " клуб"]);
    tags
}
