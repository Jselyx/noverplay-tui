use crate::app::Screen;

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
    OpenHelp,
    OpenCommandPalette,
    CloseModal,
    AcceptOnboarding,
    Tick,
    Resize,
}
