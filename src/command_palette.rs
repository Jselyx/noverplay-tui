#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaletteCommand {
    Search,
    ImportPlaylist,
    Profile,
    Settings,
    ProbeSoundCloud,
}

impl PaletteCommand {
    pub const ALL: [Self; 5] = [
        Self::Search,
        Self::ImportPlaylist,
        Self::Profile,
        Self::Settings,
        Self::ProbeSoundCloud,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Search => "Поиск",
            Self::ImportPlaylist => "Импорт плейлиста",
            Self::Profile => "Профиль и аккаунт",
            Self::Settings => "Настроить сервисы",
            Self::ProbeSoundCloud => "Проверить доступ к SoundCloud",
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommandPalette {
    pub selected: usize,
}

impl CommandPalette {
    pub fn next(&mut self) {
        self.selected = (self.selected + 1).min(PaletteCommand::ALL.len() - 1);
    }

    pub fn previous(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn command(&self) -> PaletteCommand {
        PaletteCommand::ALL[self.selected.min(PaletteCommand::ALL.len() - 1)]
    }
}
