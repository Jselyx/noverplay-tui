use std::sync::Arc;

use tokio::{sync::mpsc::UnboundedSender, task::JoinHandle};

use crate::{
    account::{
        client::AccountClient,
        error::{AccountApiError, AccountResult},
        models::{AccountAction, AccountSession, CaptchaSolution},
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
    secrets
        .remove(SecretKey::SessionToken)
        .map_err(|error| AccountApiError::local("SESSION_REMOVE", error.to_string()))?;
    remote_result
}
