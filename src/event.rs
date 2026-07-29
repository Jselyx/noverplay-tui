use std::time::Duration;

use crossterm::event::{Event as CrosstermEvent, EventStream, KeyCode, KeyEvent, KeyModifiers};
use futures_util::StreamExt;
use tokio::time::{Interval, interval};

use crate::{action::Action, app::Screen};

pub struct EventPump {
    terminal: EventStream,
    tick: Interval,
}

impl EventPump {
    pub fn new() -> Self {
        Self {
            terminal: EventStream::new(),
            tick: interval(Duration::from_secs(1)),
        }
    }

    pub async fn next(&mut self, search_mode: bool, modal_open: bool) -> Action {
        tokio::select! {
            _ = self.tick.tick() => Action::Tick,
            event = self.terminal.next() => {
                match event {
                    Some(Ok(CrosstermEvent::Key(key))) if key.is_press() => {
                        map_key(key, search_mode, modal_open)
                    }
                    Some(Ok(CrosstermEvent::Resize(_, _))) => Action::Resize,
                    Some(Ok(CrosstermEvent::Mouse(mouse))) => match mouse.kind {
                        crossterm::event::MouseEventKind::ScrollUp => Action::SelectPrevious,
                        crossterm::event::MouseEventKind::ScrollDown => Action::SelectNext,
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

fn map_key(key: KeyEvent, search_mode: bool, modal_open: bool) -> Action {
    // да тут много клавиш, терминал сам их телепатией не распарсит АЛЛООООО 🤡
    if modal_open {
        return match key.code {
            KeyCode::Esc => Action::CloseModal,
            KeyCode::Enter => Action::AcceptOnboarding,
            _ => Action::Resize,
        };
    }
    if search_mode {
        return match key.code {
            KeyCode::Esc => Action::Navigate(Screen::Home),
            KeyCode::Enter => Action::SubmitSearch,
            KeyCode::Backspace => Action::SearchBackspace,
            KeyCode::Up => Action::SelectPrevious,
            KeyCode::Down => Action::SelectNext,
            KeyCode::Char(value) => Action::SearchInput(value),
            _ => Action::Resize,
        };
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('k') {
        return Action::OpenCommandPalette;
    }
    match key.code {
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Char('?') => Action::OpenHelp,
        KeyCode::Char('/') => Action::StartSearch,
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
        KeyCode::Enter => Action::Activate,
        _ => Action::Resize,
    }
}
