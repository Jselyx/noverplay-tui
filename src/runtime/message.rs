use crate::{
    model::{PlaybackSource, TrackRef},
    onboarding::{
        SoundCloudAccess,
        zapret::{ZapretApplyResult, ZapretPlan},
    },
};

pub(super) enum RuntimeMessage {
    SearchFinished {
        generation: u64,
        query: String,
        tracks: Vec<TrackRef>,
        failures: Vec<String>,
    },
    WaveFinished {
        generation: u64,
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
    SoundCloudChecked {
        generation: u64,
        access: SoundCloudAccess,
    },
    ZapretPlanned {
        generation: u64,
        result: Result<Box<ZapretPlan>, String>,
    },
    ZapretApplied {
        generation: u64,
        result: Result<ZapretApplyResult, String>,
    },
}
