use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use tokio::{sync::mpsc::UnboundedSender, task::JoinHandle};

use crate::{
    account::{
        client::AccountClient,
        error::{AccountApiError, AccountResult},
        models::{AccountAction, AccountSession, BootstrapUpdate, CaptchaSolution},
    },
    secrets::{SecretKey, SecretStore},
};

use super::message::RuntimeMessage;

pub(super) fn spawn_captcha(
    client: Arc<AccountClient>,
    sender: UnboundedSender<RuntimeMessage>,
    generation: u64,
    action: AccountAction,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let result = client.captcha(action).await;
        let _ = sender.send(RuntimeMessage::AccountCaptcha {
            generation,
            action,
            result,
        });
    })
}

pub(super) struct AuthenticationRequest {
    pub action: AccountAction,
    pub username: String,
    pub password: String,
    pub captcha_id: String,
    pub solution: CaptchaSolution,
}

pub(super) fn spawn_authentication(
    client: Arc<AccountClient>,
    secrets: SecretStore,
    sender: UnboundedSender<RuntimeMessage>,
    generation: u64,
    request: AuthenticationRequest,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let result = authenticate(&client, &secrets, request).await;
        let _ = sender.send(RuntimeMessage::AccountAuthenticated { generation, result });
    })
}

pub(super) fn spawn_restore(
    client: Arc<AccountClient>,
    secrets: SecretStore,
    sender: UnboundedSender<RuntimeMessage>,
    generation: u64,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let result = restore(&client, &secrets).await;
        let _ = sender.send(RuntimeMessage::AccountRestored { generation, result });
    })
}

pub(super) fn spawn_logout(
    client: Arc<AccountClient>,
    secrets: SecretStore,
    sender: UnboundedSender<RuntimeMessage>,
    generation: u64,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let result = logout(&client, &secrets).await;
        let _ = sender.send(RuntimeMessage::AccountLoggedOut { generation, result });
    })
}

pub(super) fn spawn_bootstrap(
    client: Arc<AccountClient>,
    secrets: SecretStore,
    sender: UnboundedSender<RuntimeMessage>,
    generation: u64,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let result = bootstrap(&client, &secrets).await;
        let _ = sender.send(RuntimeMessage::BootstrapFinished { generation, result });
    })
}

async fn authenticate(
    client: &AccountClient,
    secrets: &SecretStore,
    request: AuthenticationRequest,
) -> AccountResult<AccountSession> {
    let response = match request.action {
        AccountAction::Login => {
            client
                .login(
                    &request.username,
                    &request.password,
                    &request.captcha_id,
                    &request.solution,
                )
                .await?
        }
        AccountAction::Register => {
            client
                .register(
                    &request.username,
                    &request.password,
                    &request.captcha_id,
                    &request.solution,
                )
                .await?
        }
    };
    secrets
        .set(SecretKey::SessionToken, &response.session_token)
        .map_err(|error| AccountApiError::local("SESSION_SAVE", error.to_string()))?;
    Ok(AccountSession {
        user: response.user,
        expires_at: response.expires_at,
    })
}

async fn restore(
    client: &AccountClient,
    secrets: &SecretStore,
) -> AccountResult<Option<AccountSession>> {
    let token = secrets
        .get(SecretKey::SessionToken)
        .map_err(|error| AccountApiError::local("SESSION_READ", error.to_string()))?;
    let Some(token) = token.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    match client.session(&token).await {
        Ok(response) => Ok(Some(AccountSession {
            user: response.user,
            expires_at: response.expires_at,
        })),
        Err(error) if matches!(error.status, Some(401 | 403)) => {
            secrets
                .remove(SecretKey::SessionToken)
                .map_err(|remove| AccountApiError::local("SESSION_REMOVE", remove.to_string()))?;
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

async fn logout(client: &AccountClient, secrets: &SecretStore) -> AccountResult<()> {
    let token = secrets
        .get(SecretKey::SessionToken)
        .map_err(|error| AccountApiError::local("SESSION_READ", error.to_string()))?;
    let remote_result = if let Some(token) = token.filter(|value| !value.trim().is_empty()) {
        client.logout(&token).await
    } else {
        Ok(())
    };
    // локальный выход важнее гордости сервера, висящий bearer на диске нам точно не нужен
    let session_remove = secrets
        .remove(SecretKey::SessionToken)
        .map_err(|error| AccountApiError::local("SESSION_REMOVE", error.to_string()));
    let cache_remove = secrets
        .remove(SecretKey::SoundCloudClientId)
        .map_err(|error| AccountApiError::local("BOOTSTRAP_REMOVE", error.to_string()));
    session_remove?;
    cache_remove?;
    remote_result
}

async fn bootstrap(
    client: &AccountClient,
    secrets: &SecretStore,
) -> AccountResult<BootstrapUpdate> {
    let token = secrets
        .get(SecretKey::SessionToken)
        .map_err(|error| AccountApiError::local("SESSION_READ", error.to_string()))?
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AccountApiError::local("SESSION_MISSING", "Сначала войди в аккаунт"))?;
    let response = client.bootstrap(&token).await?;
    if response.protocol != "noverplay-app-auth-v2" || response.client_kind != "tui" {
        return Err(AccountApiError::local(
            "BOOTSTRAP_PROTOCOL",
            "Сервер вернул bootstrap для другого клиента",
        ));
    }
    if response.soundcloud.client_id.trim().is_empty() {
        return Err(AccountApiError::local(
            "BOOTSTRAP_EMPTY",
            "Сервер не выдал SoundCloud client_id",
        ));
    }
    secrets
        .set(
            SecretKey::SoundCloudClientId,
            response.soundcloud.client_id.trim(),
        )
        .map_err(|error| AccountApiError::local("BOOTSTRAP_SAVE", error.to_string()))?;
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default();
    let refresh_after_ms = response
        .refresh_after_seconds
        .clamp(60, 86_400)
        .saturating_mul(1_000);
    Ok(BootstrapUpdate {
        protocol: response.protocol,
        refresh_at: response.refresh_at,
        refresh_at_ms: now_ms.saturating_add(refresh_after_ms),
    })
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    use super::*;

    #[tokio::test]
    async fn bootstrap_saves_server_cache_without_eating_manual_override() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut bytes = [0_u8; 16_384];
            let read = socket.read(&mut bytes).unwrap();
            let request = String::from_utf8_lossy(&bytes[..read]).to_ascii_lowercase();
            assert!(request.starts_with("get /api/tui/bootstrap http/1.1"));
            assert!(request.contains("ya-ne-hkamori: bearer session-token"));
            let body = r#"{"protocol":"noverplay-app-auth-v2","client_kind":"tui","refresh_after_seconds":3600,"refresh_at":"later","soundcloud":{"client_id":"server-client-id"}}"#;
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let temp = tempfile::tempdir().unwrap();
        let secrets = SecretStore::file_only(temp.path().join("secrets.json"));
        secrets
            .set(SecretKey::SessionToken, "session-token")
            .unwrap();
        secrets
            .set(SecretKey::SoundCloudClientIdOverride, "manual-client-id")
            .unwrap();
        let client = AccountClient::new(&format!("http://{address}"), &secrets).unwrap();

        let update = bootstrap(&client, &secrets).await.unwrap();

        server.join().unwrap();
        assert_eq!(update.protocol, "noverplay-app-auth-v2");
        assert_eq!(update.refresh_at, "later");
        assert_eq!(
            secrets
                .get(SecretKey::SoundCloudClientId)
                .unwrap()
                .as_deref(),
            Some("server-client-id")
        );
        assert_eq!(
            secrets
                .get(SecretKey::SoundCloudClientIdOverride)
                .unwrap()
                .as_deref(),
            Some("manual-client-id")
        );
    }

    #[tokio::test]
    async fn local_logout_drops_server_cache_but_respects_own_key() {
        let temp = tempfile::tempdir().unwrap();
        let secrets = SecretStore::file_only(temp.path().join("secrets.json"));
        secrets
            .set(SecretKey::SoundCloudClientId, "server-client-id")
            .unwrap();
        secrets
            .set(SecretKey::SoundCloudClientIdOverride, "manual-client-id")
            .unwrap();
        let client = AccountClient::new("http://127.0.0.1:9", &secrets).unwrap();

        logout(&client, &secrets).await.unwrap();

        assert_eq!(secrets.get(SecretKey::SoundCloudClientId).unwrap(), None);
        assert_eq!(
            secrets
                .get(SecretKey::SoundCloudClientIdOverride)
                .unwrap()
                .as_deref(),
            Some("manual-client-id")
        );
    }
}
