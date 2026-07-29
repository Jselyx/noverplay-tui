use crate::model::{PlaybackSource, TrackRef};

pub(super) enum RuntimeMessage {
    SearchFinished {
        generation: u64,
        query: String,
        tracks: Vec<TrackRef>,
        failures: Vec<String>,
    },
    PlaybackReady {
        generation: u64,
        source: PlaybackSource,
    },
    PlaybackFailed {
        generation: u64,
        error: String,
    },
}
