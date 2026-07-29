use crate::{
    config::AppConfig,
    provider::{ProviderRegistry, soundcloud::SoundCloudProvider, yandex::YandexProvider},
    secrets::{SecretKey, SecretStore},
};

pub(super) struct ProviderSetup {
    pub(super) registry: ProviderRegistry,
    pub(super) notices: Vec<String>,
}

pub(super) fn build_registry(config: &AppConfig, secrets: &SecretStore) -> ProviderSetup {
    let mut registry = ProviderRegistry::default();
    let mut notices = Vec::new();

    let soundcloud_key = config
        .soundcloud_client_id_override
        .clone()
        .or_else(|| load_secret(secrets, SecretKey::SoundCloudClientId, &mut notices));
    if let Some(client_id) = soundcloud_key.filter(|value| !value.trim().is_empty()) {
        match SoundCloudProvider::new(client_id) {
            Ok(provider) => registry.register(provider),
            Err(error) => notices.push(format!("SoundCloud не настроен: {error}")),
        }
    }

    if let Some(token) = load_secret(secrets, SecretKey::YandexToken, &mut notices)
        .filter(|value| !value.trim().is_empty())
    {
        match YandexProvider::new(token) {
            Ok(provider) => registry.register(provider),
            Err(error) => notices.push(format!("Yandex Music не настроен: {error}")),
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
