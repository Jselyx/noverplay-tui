use std::collections::HashMap;

use crate::model::TrackRef;

use super::{WaveCandidate, WaveCandidateOrigin, track_key};

#[derive(Default)]
pub(crate) struct CandidatePool {
    candidates: HashMap<String, WaveCandidate>,
}

impl CandidatePool {
    pub fn len(&self) -> usize {
        self.candidates.len()
    }

    pub fn insert(&mut self, track: TrackRef, origin: WaveCandidateOrigin) {
        let key = track_key(&track);
        if key.is_empty() {
            return;
        }
        self.candidates
            .entry(key)
            .and_modify(|candidate| candidate.add_origin(origin))
            .or_insert_with(|| WaveCandidate::new(track, origin));
    }

    pub fn extend(
        &mut self,
        tracks: impl IntoIterator<Item = TrackRef>,
        origin: WaveCandidateOrigin,
        target: usize,
    ) {
        for track in tracks {
            self.insert(track, origin);
            if self.len() >= target {
                break;
            }
        }
    }

    pub fn into_candidates(self) -> Vec<WaveCandidate> {
        self.candidates.into_values().collect()
    }
}
