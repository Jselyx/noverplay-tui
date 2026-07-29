use std::path::PathBuf;

use crate::{model::TrackRef, onboarding::zapret::ZapretPlan};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppEffect {
    Search {
        query: String,
        immediate: bool,
    },
    GenerateWave,
    ProbeSoundCloud,
    PlanZapret(PathBuf),
    ApplyZapret(Box<ZapretPlan>),
    SelectAudioOutput {
        output: Option<String>,
        volume_percent: u8,
    },
    Play(Box<TrackRef>),
    Pause,
    Resume,
    Seek(u64),
    SetVolume(u8),
    Stop,
}
