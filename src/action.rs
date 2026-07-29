use crate::{app::Screen, audio::AudioEvent, model::TrackRef};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Action {
    Quit,
    Navigate(Screen),
    SelectPrevious,
    SelectNext,
    Activate,
    TogglePause,
    NextTrack,
    PreviousTrack,
    ChangeVolume(i8),
    Seek(i64),
    ToggleShuffle,
    CycleRepeat,
    StartSearch,
    SearchInput(char),
    SearchBackspace,
    SubmitSearch,
    SearchFinished {
        query: String,
        tracks: Vec<TrackRef>,
        failures: Vec<String>,
    },
    AudioProgress {
        position_ms: u64,
        buffered_ms: u64,
    },
    Audio(AudioEvent),
    PlaybackFailed(String),
    OpenHelp,
    OpenCommandPalette,
    CloseModal,
    AcceptOnboarding,
    Tick,
    Resize,
}
