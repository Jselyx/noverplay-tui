use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub config_file: PathBuf,
    pub database_file: PathBuf,
    pub secrets_file: PathBuf,
    pub control_endpoint_file: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Result<Self> {
        let dirs = ProjectDirs::from("dev", "Noverplay", "noverplay-tui")
            .context("Не удалось определить каталог данных пользователя")?;
        Ok(Self::from_roots(
            dirs.config_dir().to_path_buf(),
            dirs.data_dir().to_path_buf(),
            dirs.cache_dir().to_path_buf(),
        ))
    }

    pub fn from_roots(config_dir: PathBuf, data_dir: PathBuf, cache_dir: PathBuf) -> Self {
        Self {
            config_file: config_dir.join("config.toml"),
            database_file: data_dir.join("library.sqlite3"),
            secrets_file: data_dir.join("secrets.json"),
            control_endpoint_file: data_dir.join("control.json"),
            config_dir,
            data_dir,
            cache_dir,
        }
    }

    pub fn ensure(&self) -> Result<()> {
        for path in [&self.config_dir, &self.data_dir, &self.cache_dir] {
            fs::create_dir_all(path)
                .with_context(|| format!("Не удалось создать {}", path.display()))?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct HotkeyBindings {
    pub play_pause: String,
    pub next: String,
    pub previous: String,
    pub volume_up: String,
    pub volume_down: String,
}

impl Default for HotkeyBindings {
    fn default() -> Self {
        Self {
            play_pause: "Ctrl+Alt+Space".to_string(),
            next: "Ctrl+Alt+Right".to_string(),
            previous: "Ctrl+Alt+Left".to_string(),
            volume_up: "Ctrl+Alt+Up".to_string(),
            volume_down: "Ctrl+Alt+Down".to_string(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct AppConfig {
    pub server_url: String,
    pub theme: String,
    pub language: String,
    pub audio_output: Option<String>,
    pub volume_percent: u8,
    pub cover_cache_mb: u16,
    pub search_debounce_ms: u64,
    pub frame_limit: u16,
    pub onboarding_completed: bool,
    pub guest_mode: bool,
    pub soundcloud_enabled: bool,
    pub yandex_enabled: bool,
    pub deezer_enabled: bool,
    pub soundcloud_client_id_override: Option<String>,
    pub soundcloud_client_id_refresh_at_ms: Option<i64>,
    pub global_hotkeys_enabled: bool,
    pub hotkeys: HotkeyBindings,
    pub keybindings_notice_seen: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server_url: "https://api.noverplay.space".to_string(),
            theme: "cyan".to_string(),
            language: "ru".to_string(),
            audio_output: None,
            volume_percent: 75,
            cover_cache_mb: 96,
            search_debounce_ms: 280,
            frame_limit: 30,
            onboarding_completed: false,
            guest_mode: false,
            soundcloud_enabled: true,
            yandex_enabled: true,
            deezer_enabled: true,
            soundcloud_client_id_override: None,
            soundcloud_client_id_refresh_at_ms: None,
            global_hotkeys_enabled: false,
            hotkeys: HotkeyBindings::default(),
            keybindings_notice_seen: false,
        }
    }
}

impl AppConfig {
    pub fn load(paths: &AppPaths) -> Result<Self> {
        if !paths.config_file.exists() {
            return Ok(Self::default());
        }
        let source = fs::read_to_string(&paths.config_file)
            .with_context(|| format!("Не удалось прочитать {}", paths.config_file.display()))?;
        toml::from_str(&source).context("Настройки повреждены")
    }

    pub fn save(&self, paths: &AppPaths) -> Result<()> {
        paths.ensure()?;
        let source = toml::to_string_pretty(self).context("Не удалось собрать настройки")?;
        fs::write(&paths.config_file, source)
            .with_context(|| format!("Не удалось сохранить {}", paths.config_file.display()))
    }

    pub fn normalized(mut self) -> Self {
        // юзер может вписать 9000 fps, но железо не обязано участвовать в этом перформансе 🤡
        self.volume_percent = self.volume_percent.min(100);
        self.cover_cache_mb = self.cover_cache_mb.clamp(16, 1024);
        self.search_debounce_ms = self.search_debounce_ms.clamp(100, 2_000);
        self.frame_limit = self.frame_limit.clamp(10, 60);
        self.server_url = self.server_url.trim_end_matches('/').to_string();
        if matches!(
            self.server_url.as_str(),
            "https://api.noverplay.ru" | "http://api.noverplay.ru"
        ) {
            // старый домен умер даже не родившись, тащить его дальше было бы некромантией для бедных
            self.server_url = "https://api.noverplay.space".to_string();
        }
        self.soundcloud_client_id_override = self
            .soundcloud_client_id_override
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_roundtrip_preserves_values() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_roots(
            temp.path().join("config"),
            temp.path().join("data"),
            temp.path().join("cache"),
        );
        let config = AppConfig {
            volume_percent: 42,
            onboarding_completed: true,
            ..AppConfig::default()
        };
        config.save(&paths).unwrap();
        let loaded = AppConfig::load(&paths).unwrap();
        assert_eq!(loaded.volume_percent, 42);
        assert!(loaded.onboarding_completed);
    }

    #[test]
    fn unsafe_config_values_are_clamped() {
        let config = AppConfig {
            volume_percent: 255,
            frame_limit: 500,
            search_debounce_ms: 1,
            ..AppConfig::default()
        }
        .normalized();
        assert_eq!(config.volume_percent, 100);
        assert_eq!(config.frame_limit, 60);
        assert_eq!(config.search_debounce_ms, 100);
    }

    #[test]
    fn dead_server_domain_is_moved_to_the_real_one() {
        let config = AppConfig {
            server_url: "https://api.noverplay.ru/".to_string(),
            ..AppConfig::default()
        }
        .normalized();
        assert_eq!(config.server_url, "https://api.noverplay.space");
    }

    #[test]
    fn hotkey_bindings_roundtrip_and_have_cross_platform_defaults() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_roots(
            temp.path().join("config"),
            temp.path().join("data"),
            temp.path().join("cache"),
        );
        let mut config = AppConfig {
            global_hotkeys_enabled: true,
            ..AppConfig::default()
        };
        config.hotkeys.play_pause = "Ctrl+Alt+P".to_string();
        config.save(&paths).unwrap();
        let loaded = AppConfig::load(&paths).unwrap();
        assert!(loaded.global_hotkeys_enabled);
        assert_eq!(loaded.hotkeys.play_pause, "Ctrl+Alt+P");
        assert!(!loaded.hotkeys.next.trim().is_empty());
    }
}
