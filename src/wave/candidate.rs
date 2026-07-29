use crate::model::TrackRef;

use super::{artist_id, text::normalize_search_text};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WaveCandidateOrigin {
    Seed,
    Related,
    Explore,
    Comfort,
    YandexPersonal,
    Backfill,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WaveBucket {
    Core,
    Related,
    Favorites,
    Discovery,
    Backfill,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WaveCandidate {
    pub track: TrackRef,
    pub origins: Vec<WaveCandidateOrigin>,
}

impl WaveCandidate {
    pub fn new(track: TrackRef, origin: WaveCandidateOrigin) -> Self {
        Self {
            track,
            origins: vec![origin],
        }
    }

    pub fn add_origin(&mut self, origin: WaveCandidateOrigin) {
        if !self.origins.contains(&origin) {
            self.origins.push(origin);
        }
    }

    pub fn bucket(&self) -> WaveBucket {
        if self.origins.contains(&WaveCandidateOrigin::Comfort) {
            WaveBucket::Favorites
        } else if self.origins.contains(&WaveCandidateOrigin::Explore) {
            WaveBucket::Discovery
        } else if self.origins.iter().any(|origin| {
            matches!(
                origin,
                WaveCandidateOrigin::Related | WaveCandidateOrigin::YandexPersonal
            )
        }) {
            WaveBucket::Related
        } else if self.origins.contains(&WaveCandidateOrigin::Backfill) {
            WaveBucket::Backfill
        } else {
            WaveBucket::Core
        }
    }

    pub fn artist_id(&self) -> String {
        artist_id(&self.track)
    }

    pub fn is_tracklike(&self) -> bool {
        is_tracklike(&self.track)
    }
}

fn is_tracklike(track: &TrackRef) -> bool {
    if track.id.trim().is_empty() || track.title.trim().is_empty() {
        return false;
    }
    if let Some(duration_ms) = track.duration_ms
        && duration_ms > 0
        && !(45_000..15 * 60_000).contains(&duration_ms)
    {
        return false;
    }
    let raw_title = track.title.trim().to_ascii_lowercase();
    let raw_artist = track.display_artist().trim().to_ascii_lowercase();
    let normalized = format!(
        " {} ",
        normalize_search_text(&format!("{} {}", track.title, track.display_artist()))
    );
    if has_any(
        &raw_title,
        &[
            "official video",
            "music video",
            "lyric video",
            "video clip",
            "visualizer",
            "(live",
            "[live",
            " live at ",
            " live from ",
            " livestream",
            " full album",
        ],
    ) || raw_artist.contains("podcast")
        || raw_artist.contains("радио")
    {
        return false;
    }
    ![
        " stream ",
        " livestream ",
        " стрим ",
        " podcast ",
        " подкаст ",
        " episode ",
        " эпизод ",
        " radio ",
        " радио ",
        " official video ",
        " music video ",
        " lyric video ",
        " video clip ",
        " visualizer ",
        " full album ",
        " live at ",
        " live from ",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn has_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}
