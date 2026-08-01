use std::time::Duration;

use crossterm::event::{
    Event as CrosstermEvent, EventStream, KeyCode, KeyEvent, KeyModifiers, MouseButton,
    MouseEventKind,
};
use futures_util::StreamExt;
use tokio::time::{Interval, interval};

use crate::{action::Action, app::Screen};

pub struct EventPump {
    terminal: EventStream,
    tick: Interval,
    terminal_size: (u16, u16),
}

impl EventPump {
    pub fn new() -> Self {
        Self::with_frame_limit(30)
    }

    pub fn with_frame_limit(frame_limit: u16) -> Self {
        let frame_limit = u64::from(frame_limit.clamp(10, 60));
        Self {
            terminal: EventStream::new(),
            tick: interval(Duration::from_millis(1_000 / frame_limit)),
            terminal_size: crossterm::terminal::size().unwrap_or((80, 24)),
        }
    }

    pub async fn next(
        &mut self,
        search_mode: bool,
        search_input_focused: bool,
        modal_open: bool,
        text_modal: bool,
        search_has_results: bool,
    ) -> Action {
        tokio::select! {
            _ = self.tick.tick() => Action::Tick,
            event = self.terminal.next() => {
                match event {
                    Some(Ok(CrosstermEvent::Key(key))) if key.is_press() => {
                        map_key(key, search_mode, search_input_focused, modal_open, text_modal, search_has_results)
                    }
                    Some(Ok(CrosstermEvent::Resize(width, height))) => {
                        self.terminal_size = (width, height);
                        Action::Resize
                    }
                    Some(Ok(CrosstermEvent::Mouse(mouse))) => match mouse.kind {
                        MouseEventKind::ScrollUp => Action::SelectPrevious,
                        MouseEventKind::ScrollDown => Action::SelectNext,
                        MouseEventKind::Down(MouseButton::Left) => Action::MouseClick {
                            column: mouse.column,
                            row: mouse.row,
                            terminal_width: self.terminal_size.0,
                            terminal_height: self.terminal_size.1,
                        },
                        _ => Action::Resize,
                    },
                    _ => Action::Tick,
                }
            }
        }
    }
}

impl Default for EventPump {
    fn default() -> Self {
        Self::new()
    }
}

fn map_key(
    key: KeyEvent,
    search_mode: bool,
    search_input_focused: bool,
    modal_open: bool,
    text_modal: bool,
    _search_has_results: bool,
) -> Action {
    // да тут много клавиш, терминал сам их телепатией не распарсит АЛЛООООО 🤡
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('9') {
        return Action::OpenKeybindings;
    }
    if modal_open {
        if text_modal {
            return match key.code {
                KeyCode::Esc => Action::CloseModal,
                KeyCode::F(2) => Action::ToggleAccountMode,
                KeyCode::Enter => Action::ModalSubmit,
                KeyCode::Tab | KeyCode::Down => Action::ModalNext,
                KeyCode::BackTab | KeyCode::Up => Action::ModalPrevious,
                KeyCode::Backspace => Action::ModalBackspace,
                KeyCode::Char(value) => Action::ModalInput(value),
                _ => Action::Resize,
            };
        }
        return match key.code {
            KeyCode::Esc => Action::CloseModal,
            KeyCode::Enter => Action::ModalSubmit,
            KeyCode::Up | KeyCode::Char('k') => Action::ModalPrevious,
            KeyCode::Down | KeyCode::Char('j') => Action::ModalNext,
            KeyCode::Char(' ') => Action::ModalToggle,
            KeyCode::Backspace => Action::ModalBackspace,
            KeyCode::Char(value) => Action::ModalInput(value),
            _ => Action::Resize,
        };
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('k') {
        return Action::OpenCommandPalette;
    }
    if search_mode {
        if key.modifiers.contains(KeyModifiers::ALT) {
            return match key.code {
                KeyCode::Char('1') => Action::Navigate(Screen::Home),
                KeyCode::Char('2') => Action::Navigate(Screen::Wave),
                KeyCode::Char('3') => Action::Navigate(Screen::Search),
                KeyCode::Char('4') => Action::Navigate(Screen::Library),
                KeyCode::Char('5') => Action::Navigate(Screen::Playlists),
                KeyCode::Char('6') => Action::Navigate(Screen::Queue),
                KeyCode::Char('7') => Action::Navigate(Screen::Profile),
                KeyCode::Char('8') => Action::Navigate(Screen::Settings),
                _ => Action::Resize,
            };
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('j') {
            return Action::ToggleSearchFocus;
        }
        if key.code == KeyCode::Tab {
            return Action::CycleSearchProvider;
        }
        if !search_input_focused {
            return match key.code {
                KeyCode::Char('/') => Action::FocusSearchInput,
                KeyCode::Enter => Action::Activate,
                KeyCode::Up | KeyCode::Char('k') => Action::SelectPrevious,
                KeyCode::Down | KeyCode::Char('j') => Action::SelectNext,
                _ => Action::Resize,
            };
        }
        return match key.code {
            KeyCode::Enter => Action::SubmitSearch,
            KeyCode::Backspace => Action::SearchBackspace,
            KeyCode::Up => Action::SelectPrevious,
            KeyCode::Down => Action::SelectNext,
            KeyCode::Char(value) => Action::SearchInput(value),
            _ => Action::Resize,
        };
    }
    match key.code {
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Char('?') => Action::OpenHelp,
        KeyCode::Char('/') => Action::StartSearch,
        KeyCode::Char('i') => Action::OpenPlaylistImport,
        KeyCode::Char('f') => Action::ToggleLike,
        KeyCode::Char('x') => Action::AccountLogout,
        KeyCode::Char(' ') => Action::TogglePause,
        KeyCode::Char('j') | KeyCode::Down => Action::SelectNext,
        KeyCode::Char('k') | KeyCode::Up => Action::SelectPrevious,
        KeyCode::Char('h') | KeyCode::Left => Action::Seek(-10_000),
        KeyCode::Char('l') | KeyCode::Right => Action::Seek(10_000),
        KeyCode::Char('n') => Action::NextTrack,
        KeyCode::Char('p') => Action::PreviousTrack,
        KeyCode::Char('+') | KeyCode::Char('=') => Action::ChangeVolume(5),
        KeyCode::Char('-') => Action::ChangeVolume(-5),
        KeyCode::Char('s') => Action::ToggleShuffle,
        KeyCode::Char('r') => Action::CycleRepeat,
        KeyCode::Char('1') => Action::Navigate(Screen::Home),
        KeyCode::Char('2') => Action::Navigate(Screen::Wave),
        KeyCode::Char('3') => Action::Navigate(Screen::Search),
        KeyCode::Char('4') => Action::Navigate(Screen::Library),
        KeyCode::Char('5') => Action::Navigate(Screen::Playlists),
        KeyCode::Char('6') => Action::Navigate(Screen::Queue),
        KeyCode::Char('7') => Action::Navigate(Screen::Profile),
        KeyCode::Char('8') => Action::Navigate(Screen::Settings),
        KeyCode::Esc => Action::Back,
        KeyCode::Enter => Action::Activate,
        _ => Action::Resize,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_activates_ready_search_result() {
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(
            map_key(key, true, false, false, false, true),
            Action::Activate
        );
        assert_eq!(
            map_key(key, true, true, false, false, false),
            Action::SubmitSearch
        );
    }

    #[test]
    fn search_can_switch_between_typing_and_navigation_without_escape() {
        let toggle = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL);
        assert_eq!(
            map_key(toggle, true, true, false, false, true),
            Action::ToggleSearchFocus
        );
        assert_eq!(
            map_key(
                KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE),
                true,
                false,
                false,
                false,
                true,
            ),
            Action::FocusSearchInput
        );
    }

    #[test]
    fn tab_cycles_search_provider_and_alt_number_navigates_away() {
        assert_eq!(
            map_key(
                KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
                true,
                true,
                false,
                false,
                false,
            ),
            Action::CycleSearchProvider
        );
        assert_eq!(
            map_key(
                KeyEvent::new(KeyCode::Char('4'), KeyModifiers::ALT),
                true,
                true,
                false,
                false,
                false,
            ),
            Action::Navigate(Screen::Library)
        );
    }

    #[test]
    fn command_palette_survives_search_input_mode() {
        let key = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL);
        assert_eq!(
            map_key(key, true, true, false, false, false),
            Action::OpenCommandPalette
        );
    }

    #[test]
    fn ctrl_nine_opens_keybindings_from_any_regular_screen() {
        let key = KeyEvent::new(KeyCode::Char('9'), KeyModifiers::CONTROL);
        assert_eq!(
            map_key(key, false, false, false, false, false),
            Action::OpenKeybindings
        );
        assert_eq!(
            map_key(key, true, true, false, false, false),
            Action::OpenKeybindings
        );
    }

    #[test]
    fn onboarding_receives_navigation_and_path_typing() {
        assert_eq!(
            map_key(
                KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
                false,
                false,
                true,
                false,
                false
            ),
            Action::ModalNext
        );
        assert_eq!(
            map_key(
                KeyEvent::new(KeyCode::Char('C'), KeyModifiers::SHIFT),
                false,
                false,
                true,
                false,
                false
            ),
            Action::ModalInput('C')
        );
    }

    #[test]
    fn text_modal_does_not_steal_j_and_k_for_fake_vim_navigation() {
        for value in ['j', 'k'] {
            assert_eq!(
                map_key(
                    KeyEvent::new(KeyCode::Char(value), KeyModifiers::NONE),
                    false,
                    false,
                    true,
                    true,
                    false,
                ),
                Action::ModalInput(value)
            );
        }
    }

    #[test]
    fn account_mode_has_a_key_that_does_not_end_up_in_the_password() {
        assert_eq!(
            map_key(
                KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE),
                false,
                false,
                true,
                true,
                false,
            ),
            Action::ToggleAccountMode
        );
    }
}
