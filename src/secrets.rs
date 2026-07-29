use std::{collections::BTreeMap, fs, path::PathBuf};

use anyhow::{Context, Result};
use keyring::Entry;
use serde::{Deserialize, Serialize};

const SERVICE_NAME: &str = "noverplay-tui";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecretKey {
    SessionToken,
    InstallationId,
    InstallationPrivateKey,
    SoundCloudClientId,
    SoundCloudClientIdOverride,
    YandexToken,
    DeezerArl,
}

impl SecretKey {
    const fn name(self) -> &'static str {
        match self {
            Self::SessionToken => "session-token",
            Self::InstallationId => "installation-id",
            Self::InstallationPrivateKey => "installation-private-key",
            Self::SoundCloudClientId => "soundcloud-client-id",
            Self::SoundCloudClientIdOverride => "soundcloud-client-id-override",
            Self::YandexToken => "yandex-token",
            Self::DeezerArl => "deezer-arl",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecretBackend {
    System,
    File,
}

#[derive(Clone, Debug)]
pub struct SecretStore {
    fallback_file: PathBuf,
    system_enabled: bool,
}

#[derive(Default, Deserialize, Serialize)]
struct SecretFile {
    values: BTreeMap<String, String>,
}

impl SecretStore {
    pub fn new(fallback_file: PathBuf) -> Self {
        Self {
            fallback_file,
            system_enabled: true,
        }
    }

    #[cfg(test)]
    pub(crate) fn file_only(fallback_file: PathBuf) -> Self {
        Self {
            fallback_file,
            system_enabled: false,
        }
    }

    pub fn set(&self, key: SecretKey, value: &str) -> Result<SecretBackend> {
        // keyring опять решил быть элитой, поэтому держим запасной выход для реального мира 🫩✌️
        if self.system_enabled
            && Entry::new(SERVICE_NAME, key.name())
                .and_then(|entry| entry.set_password(value))
                .is_ok()
        {
            self.remove_file_value(key)?;
            return Ok(SecretBackend::System);
        }
        self.set_file_value(key, value)?;
        Ok(SecretBackend::File)
    }

    pub fn get(&self, key: SecretKey) -> Result<Option<String>> {
        if self.system_enabled
            && let Ok(value) =
                Entry::new(SERVICE_NAME, key.name()).and_then(|entry| entry.get_password())
            && !value.is_empty()
        {
            return Ok(Some(value));
        }
        Ok(self.load_file()?.values.get(key.name()).cloned())
    }

    pub fn remove(&self, key: SecretKey) -> Result<()> {
        if self.system_enabled
            && let Ok(entry) = Entry::new(SERVICE_NAME, key.name())
        {
            let _ = entry.delete_credential();
        }
        self.remove_file_value(key)
    }

    fn load_file(&self) -> Result<SecretFile> {
        if !self.fallback_file.exists() {
            return Ok(SecretFile::default());
        }
        let source = fs::read_to_string(&self.fallback_file).with_context(|| {
            format!(
                "Не удалось прочитать локальные секреты {}",
                self.fallback_file.display()
            )
        })?;
        serde_json::from_str(&source).context("Локальные секреты повреждены")
    }

    fn set_file_value(&self, key: SecretKey, value: &str) -> Result<()> {
        let mut file = self.load_file()?;
        file.values
            .insert(key.name().to_string(), value.to_string());
        self.save_file(&file)
    }

    fn remove_file_value(&self, key: SecretKey) -> Result<()> {
        let mut file = self.load_file()?;
        if file.values.remove(key.name()).is_some() {
            self.save_file(&file)?;
        }
        Ok(())
    }

    fn save_file(&self, value: &SecretFile) -> Result<()> {
        // хахах найс секреты без родительской папки, щас бы файловую систему силой мысли создать )))))
        if let Some(parent) = self.fallback_file.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Не удалось создать каталог секретов {}", parent.display())
            })?;
        }
        let source = serde_json::to_vec(value).context("Не удалось собрать локальные секреты")?;
        write_private(&self.fallback_file, &source)
    }
}

#[cfg(unix)]
fn write_private(path: &std::path::Path, value: &[u8]) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("Не удалось открыть {}", path.display()))?;
    std::io::Write::write_all(&mut file, value)
        .with_context(|| format!("Не удалось сохранить {}", path.display()))
}

#[cfg(not(unix))]
fn write_private(path: &std::path::Path, value: &[u8]) -> Result<()> {
    fs::write(path, value).with_context(|| format!("Не удалось сохранить {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_fallback_roundtrip_works() {
        let temp = tempfile::tempdir().unwrap();
        let store = SecretStore::file_only(temp.path().join("secrets.json"));
        assert_eq!(
            store.set(SecretKey::YandexToken, "token").unwrap(),
            SecretBackend::File
        );
        assert_eq!(
            store.get(SecretKey::YandexToken).unwrap().as_deref(),
            Some("token")
        );
        store.remove(SecretKey::YandexToken).unwrap();
        assert_eq!(store.get(SecretKey::YandexToken).unwrap(), None);
    }
}
