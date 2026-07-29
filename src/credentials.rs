use anyhow::Result;

use crate::secrets::{SecretKey, SecretStore};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialKind {
    SoundCloudClientId,
    YandexToken,
}

impl CredentialKind {
    pub const ALL: [Self; 2] = [Self::SoundCloudClientId, Self::YandexToken];

    pub const fn label(self) -> &'static str {
        match self {
            Self::SoundCloudClientId => "SoundCloud client_id",
            Self::YandexToken => "Yandex OAuth токен",
        }
    }

    pub const fn secret_key(self) -> SecretKey {
        match self {
            Self::SoundCloudClientId => SecretKey::SoundCloudClientId,
            Self::YandexToken => SecretKey::YandexToken,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CredentialState {
    pub soundcloud: bool,
    pub yandex: bool,
}

impl CredentialState {
    pub fn load(secrets: &SecretStore) -> Result<Self> {
        Ok(Self {
            soundcloud: has_value(secrets, SecretKey::SoundCloudClientId)?,
            yandex: has_value(secrets, SecretKey::YandexToken)?,
        })
    }

    pub const fn is_configured(self, kind: CredentialKind) -> bool {
        match kind {
            CredentialKind::SoundCloudClientId => self.soundcloud,
            CredentialKind::YandexToken => self.yandex,
        }
    }

    pub fn set_configured(&mut self, kind: CredentialKind, configured: bool) {
        match kind {
            CredentialKind::SoundCloudClientId => self.soundcloud = configured,
            CredentialKind::YandexToken => self.yandex = configured,
        }
    }
}

fn has_value(secrets: &SecretStore, key: SecretKey) -> Result<bool> {
    Ok(secrets
        .get(key)?
        .is_some_and(|value| !value.trim().is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_only_marks_non_empty_secrets_as_ready() {
        let temp = tempfile::tempdir().unwrap();
        let secrets = SecretStore::file_only(temp.path().join("secrets.json"));
        secrets
            .set(SecretKey::SoundCloudClientId, "client-id")
            .unwrap();

        let state = CredentialState::load(&secrets).unwrap();

        assert!(state.soundcloud);
        assert!(!state.yandex);
    }
}
