use crate::{
    account::{
        error::AccountApiError,
        models::{AccountAction, AccountSession, BootstrapUpdate, CaptchaChallenge},
    },
    model::{PlaybackSource, Playlist, TrackRef},
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
    ControlSearchFinished {
        generation: u64,
        tracks: Vec<TrackRef>,
        failures: Vec<String>,
    },
    WaveFinished {
        generation: u64,
        tracks: Vec<TrackRef>,
        failures: Vec<String>,
    },
    ControlWaveFinished {
        generation: u64,
        tracks: Vec<TrackRef>,
        failures: Vec<String>,
    },
    PlaylistImported {
        generation: u64,
        result: Result<Playlist, String>,
    },
    AccountCaptcha {
        generation: u64,
        action: AccountAction,
        result: Result<CaptchaChallenge, AccountApiError>,
    },
    AccountAuthenticated {
        generation: u64,
        result: Result<AccountSession, AccountApiError>,
    },
    AccountRestored {
        generation: u64,
        result: Result<Option<AccountSession>, AccountApiError>,
    },
    BootstrapFinished {
        generation: u64,
        result: Result<BootstrapUpdate, AccountApiError>,
    },
    AccountLoggedOut {
        generation: u64,
        result: Result<(), AccountApiError>,
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
