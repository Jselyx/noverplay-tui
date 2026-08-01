use anyhow::Result;

use crate::secrets::{SecretKey, SecretStore};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialKind {
    SoundCloudClientId,
    YandexToken,
    DeezerArl,
}

impl CredentialKind {
    pub const ALL: [Self; 3] = [Self::SoundCloudClientId, Self::YandexToken, Self::DeezerArl];

    pub const fn label(self) -> &'static str {
        match self {
            Self::SoundCloudClientId => "SoundCloud client_id",
            Self::YandexToken => "Yandex OAuth токен",
            Self::DeezerArl => "Deezer ARL cookie",
        }
    }

    pub const fn secret_key(self) -> SecretKey {
        match self {
            Self::SoundCloudClientId => SecretKey::SoundCloudClientIdOverride,
            Self::YandexToken => SecretKey::YandexToken,
            Self::DeezerArl => SecretKey::DeezerArl,
        }
    }

    pub const fn hint(self) -> &'static str {
        match self {
            Self::SoundCloudClientId => {
                "После входа ключ приходит сам, здесь можно вставить собственный client_id"
            }
            Self::YandexToken => {
                "Вставь OAuth из расширения yandex-music-token, токен останется только локально"
            }
            Self::DeezerArl => {
                "Вставь значение cookie arl или строку arl=...; cookie останется только локально"
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CredentialState {
    pub soundcloud: bool,
    pub yandex: bool,
    pub deezer: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CredentialEditor {
    pub kind: CredentialKind,
    pub value: String,
    pub saving: bool,
}

impl CredentialEditor {
    pub fn new(kind: CredentialKind) -> Self {
        Self {
            kind,
            value: String::new(),
            saving: false,
        }
    }

    pub fn input(&mut self, value: char) {
        if !self.saving && !value.is_control() && self.value.chars().count() < 512 {
            self.value.push(value);
        }
    }

    pub fn backspace(&mut self) {
        if !self.saving {
            self.value.pop();
        }
    }
}

impl CredentialState {
    pub fn load(secrets: &SecretStore) -> Result<Self> {
        Ok(Self {
            soundcloud: has_value(secrets, SecretKey::SoundCloudClientIdOverride)?
                || has_value(secrets, SecretKey::SoundCloudClientId)?,
            yandex: has_value(secrets, SecretKey::YandexToken)?,
            deezer: has_value(secrets, SecretKey::DeezerArl)?,
        })
    }

    pub const fn is_configured(self, kind: CredentialKind) -> bool {
        match kind {
            CredentialKind::SoundCloudClientId => self.soundcloud,
            CredentialKind::YandexToken => self.yandex,
            CredentialKind::DeezerArl => self.deezer,
        }
    }

    pub fn set_configured(&mut self, kind: CredentialKind, configured: bool) {
        match kind {
            CredentialKind::SoundCloudClientId => self.soundcloud = configured,
            CredentialKind::YandexToken => self.yandex = configured,
            CredentialKind::DeezerArl => self.deezer = configured,
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
            .set(SecretKey::SoundCloudClientIdOverride, "client-id")
            .unwrap();

        let state = CredentialState::load(&secrets).unwrap();

        assert!(state.soundcloud);
        assert!(!state.yandex);
    }

    #[test]
    fn editor_hides_from_control_characters_and_runaway_paste() {
        let mut editor = CredentialEditor::new(CredentialKind::YandexToken);
        editor.input('\n');
        for _ in 0..600 {
            editor.input('x');
        }

        assert_eq!(editor.value.len(), 512);
        editor.backspace();
        assert_eq!(editor.value.len(), 511);
    }
}
