use std::path::PathBuf;

use crate::{credentials::CredentialKind, model::TrackRef, onboarding::zapret::ZapretPlan};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppEffect {
    Search {
        query: String,
        immediate: bool,
    },
    GenerateWave,
    SaveCredential {
        kind: CredentialKind,
        value: String,
    },
    ImportPlaylist(String),
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
