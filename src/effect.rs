use std::path::PathBuf;

use crate::{
    account::models::{AccountAction, CaptchaSolution},
    credentials::CredentialKind,
    model::{SearchProvider, TrackRef},
    onboarding::zapret::ZapretPlan,
};

#[derive(Clone, Debug, PartialEq)]
pub enum AppEffect {
    Search {
        query: String,
        provider: SearchProvider,
        immediate: bool,
    },
    ControlSearch {
        query: String,
        provider: SearchProvider,
    },
    GenerateWave,
    ControlWave,
    SetLiked {
        track: Box<TrackRef>,
        liked: bool,
    },
    SaveCredential {
        kind: CredentialKind,
        value: String,
    },
    ImportPlaylist(String),
    LoadAccountCaptcha(AccountAction),
    AuthenticateAccount {
        action: AccountAction,
        username: String,
        password: String,
        captcha_id: String,
        solution: CaptchaSolution,
    },
    RestoreAccount,
    RefreshBootstrap,
    LogoutAccount,
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
