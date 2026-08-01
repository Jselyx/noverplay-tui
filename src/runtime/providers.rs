use crate::{
    config::AppConfig,
    provider::{
        ProviderRegistry, deezer::DeezerProvider, soundcloud::SoundCloudProvider,
        yandex::YandexProvider,
    },
    secrets::{SecretKey, SecretStore},
};

pub(super) struct ProviderSetup {
    pub(super) registry: ProviderRegistry,
    pub(super) notices: Vec<String>,
}

pub(super) fn build_registry(config: &AppConfig, secrets: &SecretStore) -> ProviderSetup {
    let mut registry = ProviderRegistry::default();
    let mut notices = Vec::new();

    if config.soundcloud_enabled {
        let soundcloud_key = config
            .soundcloud_client_id_override
            .clone()
            .or_else(|| load_secret(secrets, SecretKey::SoundCloudClientIdOverride, &mut notices))
            .or_else(|| load_secret(secrets, SecretKey::SoundCloudClientId, &mut notices));
        if let Some(client_id) = soundcloud_key.filter(|value| !value.trim().is_empty()) {
            match SoundCloudProvider::new(client_id) {
                Ok(provider) => registry.register(provider),
                Err(error) => notices.push(format!("SoundCloud не настроен: {error}")),
            }
        }
    }

    if config.yandex_enabled
        && let Some(token) = load_secret(secrets, SecretKey::YandexToken, &mut notices)
            .filter(|value| !value.trim().is_empty())
    {
        match YandexProvider::new(token) {
            Ok(provider) => registry.register(provider),
            Err(error) => notices.push(format!("Yandex Music не настроен: {error}")),
        }
    }

    if config.deezer_enabled
        && let Some(arl) = load_secret(secrets, SecretKey::DeezerArl, &mut notices)
            .filter(|value| !value.trim().is_empty())
    {
        match DeezerProvider::new(arl) {
            Ok(provider) => registry.register(provider),
            Err(error) => notices.push(format!("Deezer не настроен: {error}")),
        }
    }

    ProviderSetup { registry, notices }
}

fn load_secret(secrets: &SecretStore, key: SecretKey, notices: &mut Vec<String>) -> Option<String> {
    match secrets.get(key) {
        Ok(value) => value,
        Err(error) => {
            // Хранилище секретов решило стать ребусом, сервис просто не включаем и живём дальше
            notices.push(format!("Не удалось прочитать локальный ключ: {error}"));
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_providers_ignore_even_existing_secrets() {
        let temp = tempfile::tempdir().unwrap();
        let secrets = SecretStore::new(temp.path().join("secrets.json"));
        secrets
            .set(SecretKey::SoundCloudClientId, "public-key")
            .unwrap();
        secrets.set(SecretKey::YandexToken, "oauth-token").unwrap();
        let config = AppConfig {
            soundcloud_enabled: false,
            yandex_enabled: false,
            ..AppConfig::default()
        };

        let setup = build_registry(&config, &secrets);

        assert!(
            setup
                .registry
                .get(crate::model::ProviderKind::SoundCloud)
                .is_none()
        );
        assert!(
            setup
                .registry
                .get(crate::model::ProviderKind::YandexMusic)
                .is_none()
        );
    }
}
