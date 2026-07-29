use std::{collections::BTreeMap, time::SystemTime};

use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signer, SigningKey};
use rand_core::OsRng;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    APP_VERSION,
    secrets::{SecretKey, SecretStore},
};

pub const PROTOCOL: &str = "noverplay-app-auth-v2";
pub const CLIENT_KIND: &str = "tui";
pub const HEADER_PROTOCOL: &str = "x-noverplay-app-auth";
pub const HEADER_CLIENT_KIND: &str = "x-noverplay-client-kind";
pub const HEADER_INSTALLATION_ID: &str = "skok-hkamori-sosal";
pub const HEADER_PUBLIC_KEY: &str = "pepe-shnele-wotofo";
pub const HEADER_TIMESTAMP: &str = "shnele-pepe";
pub const HEADER_NONCE: &str = "gde-dengi-lebowski";
pub const HEADER_CLIENT_VERSION: &str = "ves-materi-gabena";
pub const HEADER_SIGNATURE: &str = "proverka-na-huesosa";

pub struct InstallationIdentity {
    installation_id: String,
    signing_key: SigningKey,
}

impl InstallationIdentity {
    pub fn load_or_create(secrets: &SecretStore) -> Result<Self> {
        let installation_id = secrets.get(SecretKey::InstallationId)?;
        let private_key = secrets.get(SecretKey::InstallationPrivateKey)?;
        if let (Some(installation_id), Some(private_key)) = (installation_id, private_key)
            && let Ok(identity) = Self::from_stored(installation_id, &private_key)
        {
            return Ok(identity);
        }

        let identity = Self {
            installation_id: Uuid::new_v4().to_string(),
            signing_key: SigningKey::generate(&mut OsRng),
        };
        secrets.set(
            SecretKey::InstallationPrivateKey,
            &URL_SAFE_NO_PAD.encode(identity.signing_key.to_bytes()),
        )?;
        secrets.set(SecretKey::InstallationId, &identity.installation_id)?;
        Ok(identity)
    }

    pub fn sign(&self, method: &str, path_query: &str, body: &str) -> Result<SignedHeaders> {
        let now_ms = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .context("системные часы отстают от эпохи Unix")?
            .as_millis() as i64;
        self.sign_at(method, path_query, body, now_ms, Uuid::new_v4().to_string())
    }

    fn sign_at(
        &self,
        method: &str,
        path_query: &str,
        body: &str,
        timestamp_ms: i64,
        nonce: String,
    ) -> Result<SignedHeaders> {
        if !path_query.starts_with('/') {
            bail!("путь подписываемого запроса должен начинаться с /")
        }
        let public_key = URL_SAFE_NO_PAD.encode(self.signing_key.verifying_key().to_bytes());
        let canonical = canonicalize(
            method,
            path_query,
            body,
            timestamp_ms,
            &nonce,
            &self.installation_id,
            &public_key,
        );
        let signature =
            URL_SAFE_NO_PAD.encode(self.signing_key.sign(canonical.as_bytes()).to_bytes());
        Ok(SignedHeaders {
            protocol: PROTOCOL.to_string(),
            client_kind: CLIENT_KIND.to_string(),
            installation_id: self.installation_id.clone(),
            public_key,
            timestamp_ms: timestamp_ms.to_string(),
            nonce,
            client_version: APP_VERSION.to_string(),
            signature,
        })
    }

    fn from_stored(installation_id: String, private_key: &str) -> Result<Self> {
        if installation_id.trim().is_empty() {
            bail!("пустой installation id")
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(private_key.trim())
            .context("повреждён закрытый ключ установки")?;
        let bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("закрытый ключ установки имеет неверную длину"))?;
        Ok(Self {
            installation_id,
            signing_key: SigningKey::from_bytes(&bytes),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignedHeaders {
    pub protocol: String,
    pub client_kind: String,
    pub installation_id: String,
    pub public_key: String,
    pub timestamp_ms: String,
    pub nonce: String,
    pub client_version: String,
    pub signature: String,
}

impl SignedHeaders {
    pub fn values(&self) -> BTreeMap<&'static str, &str> {
        BTreeMap::from([
            (HEADER_PROTOCOL, self.protocol.as_str()),
            (HEADER_CLIENT_KIND, self.client_kind.as_str()),
            (HEADER_INSTALLATION_ID, self.installation_id.as_str()),
            (HEADER_PUBLIC_KEY, self.public_key.as_str()),
            (HEADER_TIMESTAMP, self.timestamp_ms.as_str()),
            (HEADER_NONCE, self.nonce.as_str()),
            (HEADER_CLIENT_VERSION, self.client_version.as_str()),
            (HEADER_SIGNATURE, self.signature.as_str()),
        ])
    }
}

fn canonicalize(
    method: &str,
    path_query: &str,
    body: &str,
    timestamp_ms: i64,
    nonce: &str,
    installation_id: &str,
    public_key: &str,
) -> String {
    let body_hash = hex::encode(Sha256::digest(body.as_bytes()));
    [
        PROTOCOL.to_string(),
        CLIENT_KIND.to_string(),
        APP_VERSION.to_string(),
        method.trim().to_ascii_uppercase(),
        path_query.to_string(),
        body_hash,
        timestamp_ms.to_string(),
        nonce.to_string(),
        installation_id.to_string(),
        public_key.to_string(),
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};

    use super::*;

    #[test]
    fn identity_survives_restart_without_changing_public_key() {
        let temp = tempfile::tempdir().unwrap();
        let secrets = SecretStore::file_only(temp.path().join("secrets.json"));
        let first = InstallationIdentity::load_or_create(&secrets).unwrap();
        let second = InstallationIdentity::load_or_create(&secrets).unwrap();

        let first = first
            .sign_at(
                "GET",
                "/api/tui/bootstrap",
                "",
                1_700_000_000_000,
                "n".into(),
            )
            .unwrap();
        let second = second
            .sign_at(
                "GET",
                "/api/tui/bootstrap",
                "",
                1_700_000_000_000,
                "n".into(),
            )
            .unwrap();

        assert_eq!(first.installation_id, second.installation_id);
        assert_eq!(first.public_key, second.public_key);
        assert_eq!(first.signature, second.signature);
    }

    #[test]
    fn signature_matches_server_v2_canonical_order() {
        let identity = InstallationIdentity {
            installation_id: "install-1".to_string(),
            signing_key: SigningKey::from_bytes(&[7; 32]),
        };
        let signed = identity
            .sign_at(
                "get",
                "/api/tui/bootstrap?fresh=1",
                "",
                1_700_000_000_000,
                "nonce-1".to_string(),
            )
            .unwrap();
        let canonical = canonicalize(
            "GET",
            "/api/tui/bootstrap?fresh=1",
            "",
            1_700_000_000_000,
            "nonce-1",
            "install-1",
            &signed.public_key,
        );
        let public = VerifyingKey::from_bytes(
            &URL_SAFE_NO_PAD
                .decode(&signed.public_key)
                .unwrap()
                .try_into()
                .unwrap(),
        )
        .unwrap();
        let signature =
            Signature::from_slice(&URL_SAFE_NO_PAD.decode(&signed.signature).unwrap()).unwrap();

        public.verify(canonical.as_bytes(), &signature).unwrap();
        assert!(
            public
                .verify("подменили путь".as_bytes(), &signature)
                .is_err()
        );
    }
}
