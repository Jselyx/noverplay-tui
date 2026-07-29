use crate::model::ProviderKind;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WaveMode {
    #[default]
    Balanced,
    Discovery,
    Favorites,
    Radio,
}

impl WaveMode {
    pub fn normalize(value: Option<&str>) -> Self {
        match value
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "discovery" | "discover" | "explore" | "new" => Self::Discovery,
            "favorites" | "favorite" | "favourites" | "likes" | "liked" => Self::Favorites,
            "radio" | "related" => Self::Radio,
            _ => Self::Balanced,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WaveMood {
    #[default]
    Auto,
    Calm,
    Focus,
    Drive,
    Night,
}

impl WaveMood {
    pub fn normalize(value: Option<&str>) -> Self {
        match value
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "calm" | "chill" | "sad" | "romantic" => Self::Calm,
            "focus" | "work" => Self::Focus,
            "drive" | "energetic" | "energy" | "workout" => Self::Drive,
            "night" | "evening" => Self::Night,
            _ => Self::Auto,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WaveSourceMode {
    #[default]
    CurrentService,
    FallbackSoft,
    LibraryOnly,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WaveTimeOfDay {
    #[default]
    Auto,
    Morning,
    Day,
    Evening,
    Night,
}

impl WaveTimeOfDay {
    pub fn normalize(value: Option<&str>) -> Self {
        match value
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "morning" => Self::Morning,
            "day" => Self::Day,
            "evening" => Self::Evening,
            "night" => Self::Night,
            _ => Self::Auto,
        }
    }
}

impl WaveSourceMode {
    pub fn normalize(value: Option<&str>) -> Self {
        match value
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "fallback_soft" | "fallback" | "mixed" | "mix" | "app" => Self::FallbackSoft,
            "library_only" | "library" | "local" => Self::LibraryOnly,
            _ => Self::CurrentService,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WaveSettings {
    pub mode: WaveMode,
    pub mood: WaveMood,
    pub time_of_day: WaveTimeOfDay,
    pub source_mode: WaveSourceMode,
    pub primary_provider: ProviderKind,
    pub size: usize,
    pub anti_repeat_hours: i64,
    pub max_plays: i64,
    pub play_window_days: i64,
    pub novelty: f64,
    pub max_artist_streak: usize,
    pub language_rotation: Vec<String>,
}

impl Default for WaveSettings {
    fn default() -> Self {
        Self {
            mode: WaveMode::Balanced,
            mood: WaveMood::Auto,
            time_of_day: WaveTimeOfDay::Auto,
            source_mode: WaveSourceMode::CurrentService,
            primary_provider: ProviderKind::YandexMusic,
            size: 40,
            anti_repeat_hours: 24,
            max_plays: 2,
            play_window_days: 7,
            novelty: 0.35,
            max_artist_streak: 2,
            language_rotation: Vec::new(),
        }
    }
}

impl WaveSettings {
    pub fn normalized(mut self, preview: bool) -> Self {
        self.size = if preview {
            self.size.clamp(6, 16)
        } else {
            self.size.clamp(10, 80)
        };
        self.anti_repeat_hours = self.anti_repeat_hours.clamp(1, 168);
        self.max_plays = self.max_plays.clamp(1, 20);
        self.play_window_days = self.play_window_days.clamp(1, 90);
        self.novelty = self.novelty.clamp(0.0, 1.0);
        self.max_artist_streak = self.max_artist_streak.clamp(1, 4);
        self.language_rotation = normalize_languages(self.language_rotation);
        self
    }

    pub fn provider_order(&self) -> Vec<ProviderKind> {
        let mut providers = vec![self.primary_provider];
        if self.source_mode == WaveSourceMode::FallbackSoft {
            for provider in [ProviderKind::YandexMusic, ProviderKind::SoundCloud] {
                if !providers.contains(&provider) {
                    providers.push(provider);
                }
            }
        }
        providers.retain(|provider| *provider != ProviderKind::Deezer);
        providers
    }
}

fn normalize_languages(values: Vec<String>) -> Vec<String> {
    let mut result = Vec::new();
    for value in values {
        let normalized = match value.trim().to_ascii_lowercase().as_str() {
            "ru" | "rus" | "russian" => "ru",
            "en" | "eng" | "english" => "en",
            _ => continue,
        };
        if !result.iter().any(|current| current == normalized) {
            result.push(normalized.to_string());
        }
    }
    result
}
