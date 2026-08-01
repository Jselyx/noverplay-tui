use std::{collections::HashMap, str::FromStr};

use anyhow::{Context, Result};
use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{Code, HotKey, Modifiers},
};

use crate::{
    action::Action,
    config::{AppConfig, HotkeyBindings},
};

pub struct GlobalHotkeys {
    _manager: GlobalHotKeyManager,
    actions: HashMap<u32, Action>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HotkeyAction {
    PlayPause,
    Next,
    Previous,
    VolumeUp,
    VolumeDown,
}

impl HotkeyAction {
    pub const ALL: [Self; 5] = [
        Self::PlayPause,
        Self::Next,
        Self::Previous,
        Self::VolumeUp,
        Self::VolumeDown,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::PlayPause => "Пауза / продолжить",
            Self::Next => "Следующий трек",
            Self::Previous => "Предыдущий трек",
            Self::VolumeUp => "Громче",
            Self::VolumeDown => "Тише",
        }
    }

    pub fn value(self, bindings: &HotkeyBindings) -> &str {
        match self {
            Self::PlayPause => &bindings.play_pause,
            Self::Next => &bindings.next,
            Self::Previous => &bindings.previous,
            Self::VolumeUp => &bindings.volume_up,
            Self::VolumeDown => &bindings.volume_down,
        }
    }

    pub fn set(self, bindings: &mut HotkeyBindings, value: String) {
        match self {
            Self::PlayPause => bindings.play_pause = value,
            Self::Next => bindings.next = value,
            Self::Previous => bindings.previous = value,
            Self::VolumeUp => bindings.volume_up = value,
            Self::VolumeDown => bindings.volume_down = value,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HotkeyEditor {
    pub action: HotkeyAction,
    pub value: String,
}

impl HotkeyEditor {
    pub fn input(&mut self, value: char) {
        if !value.is_control() && self.value.chars().count() < 64 {
            self.value.push(value);
        }
    }

    pub fn backspace(&mut self) {
        self.value.pop();
    }
}

impl GlobalHotkeys {
    pub fn new(config: &AppConfig) -> Result<Option<Self>> {
        if !config.global_hotkeys_enabled {
            return Ok(None);
        }
        let manager = GlobalHotKeyManager::new().context("глобальные хоткеи недоступны")?;
        let mut actions = HashMap::new();
        for (binding, action) in configured_bindings(&config.hotkeys) {
            let hotkey = parse_hotkey(binding)
                .with_context(|| format!("неверный глобальный хоткей: {binding}"))?;
            manager
                .register(hotkey)
                .with_context(|| format!("не удалось зарегистрировать {binding}"))?;
            actions.insert(hotkey.id(), action);
        }
        Ok(Some(Self {
            _manager: manager,
            actions,
        }))
    }

    pub fn try_action(&self) -> Option<Action> {
        let receiver = GlobalHotKeyEvent::receiver();
        while let Ok(event) = receiver.try_recv() {
            if event.state == HotKeyState::Pressed
                && let Some(action) = self.actions.get(&event.id)
            {
                return Some(action.clone());
            }
        }
        None
    }
}

fn configured_bindings(bindings: &HotkeyBindings) -> [(&str, Action); 5] {
    [
        (&bindings.play_pause, Action::TogglePause),
        (&bindings.next, Action::NextTrack),
        (&bindings.previous, Action::PreviousTrack),
        (&bindings.volume_up, Action::ChangeVolume(5)),
        (&bindings.volume_down, Action::ChangeVolume(-5)),
    ]
}

fn parse_hotkey(value: &str) -> Result<HotKey> {
    let parts = value
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let (key, modifiers) = parts.split_last().context("пустая комбинация")?;
    let mut mods = Modifiers::empty();
    for modifier in modifiers {
        match modifier.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "alt" => mods |= Modifiers::ALT,
            "shift" => mods |= Modifiers::SHIFT,
            "super" | "win" | "meta" => mods |= Modifiers::SUPER,
            other => anyhow::bail!("неизвестный модификатор {other}"),
        }
    }
    let code = code_from_name(key)?;
    Ok(HotKey::new(Some(mods), code))
}

pub fn validate_binding(value: &str) -> Result<()> {
    parse_hotkey(value).map(|_| ())
}

fn code_from_name(value: &str) -> Result<Code> {
    let normalized = match value.to_ascii_lowercase().as_str() {
        "space" => "Space",
        "left" => "ArrowLeft",
        "right" => "ArrowRight",
        "up" => "ArrowUp",
        "down" => "ArrowDown",
        _ if value.len() == 1 && value.as_bytes()[0].is_ascii_alphabetic() => {
            return Code::from_str(&format!("Key{}", value.to_ascii_uppercase()))
                .map_err(|_| anyhow::anyhow!("неизвестная клавиша {value}"));
        }
        _ => value,
    };
    Code::from_str(normalized).map_err(|_| anyhow::anyhow!("неизвестная клавиша {value}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_accepts_defaults_and_user_binding() {
        for value in ["Ctrl+Alt+Space", "Ctrl+Alt+Right", "Ctrl+Alt+P"] {
            assert!(parse_hotkey(value).is_ok(), "{value}");
        }
    }

    #[test]
    fn parser_rejects_unknown_modifier() {
        assert!(parse_hotkey("Potato+P").is_err());
    }
}
