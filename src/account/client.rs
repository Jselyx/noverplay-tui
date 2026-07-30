use std::time::Duration;

use reqwest::{Client, Method, Response, StatusCode};
use serde::{Serialize, de::DeserializeOwned};
use url::Url;

use crate::{APP_VERSION, secrets::SecretStore};

use super::{
    app_auth::InstallationIdentity,
    error::{AccountApiError, AccountResult},
    models::{
        AccountAction, AuthBody, BootstrapResponse, CaptchaChallenge, CaptchaSolution,
        ErrorEnvelope, LoginResponse, SessionResponse,
    },
};

const SESSION_HEADER: &str = "ya-ne-hkamori";

pub struct AccountClient {
    http: Client,
    base_url: Url,
    identity: InstallationIdentity,
}

impl AccountClient {
    pub fn new(server_url: &str, secrets: &SecretStore) -> anyhow::Result<Self> {
        let base_url = Url::parse(server_url.trim())?;
        let http = Client::builder()
            .user_agent(format!("noverplay-tui/{APP_VERSION}"))
            .timeout(Duration::from_secs(20))
            .build()?;
        Ok(Self {
            http,
            base_url,
            identity: InstallationIdentity::load_or_create(secrets)?,
        })
    }

    pub async fn captcha(&self, action: AccountAction) -> AccountResult<CaptchaChallenge> {
        let mut url = self.endpoint("/api/auth/captcha")?;
        url.query_pairs_mut().append_pair("action", action.as_str());
        self.send(Method::GET, url, None::<&()>, None).await
    }

    pub async fn login(
        &self,
        username: &str,
        password: &str,
        captcha_id: &str,
        solution: &CaptchaSolution,
    ) -> AccountResult<LoginResponse> {
        self.authenticate(
            AccountAction::Login,
            username,
            password,
            captcha_id,
            solution,
        )
        .await
    }

    pub async fn register(
        &self,
        username: &str,
        password: &str,
        captcha_id: &str,
        solution: &CaptchaSolution,
    ) -> AccountResult<LoginResponse> {
        self.authenticate(
            AccountAction::Register,
            username,
            password,
            captcha_id,
            solution,
        )
        .await
    }

    pub async fn session(&self, token: &str) -> AccountResult<SessionResponse> {
        self.send(
            Method::GET,
            self.endpoint("/api/auth/session")?,
            None::<&()>,
            Some(token),
        )
        .await
    }

    pub async fn logout(&self, token: &str) -> AccountResult<()> {
        #[derive(serde::Deserialize)]
        struct OkResponse {
            #[allow(dead_code)]
            ok: bool,
        }
        self.send::<_, OkResponse>(
            Method::POST,
            self.endpoint("/api/auth/logout")?,
            Some(&serde_json::json!({})),
            Some(token),
        )
        .await
        .map(|_| ())
    }

    pub async fn bootstrap(&self, token: &str) -> AccountResult<BootstrapResponse> {
        self.send(
            Method::GET,
            self.endpoint("/api/tui/bootstrap")?,
            None::<&()>,
            Some(token),
        )
        .await
    }

    async fn authenticate(
        &self,
        action: AccountAction,
        username: &str,
        password: &str,
        captcha_id: &str,
        solution: &CaptchaSolution,
    ) -> AccountResult<LoginResponse> {
        let path = match action {
            AccountAction::Login => "/api/auth/login",
            AccountAction::Register => "/api/auth/register",
        };
        self.send(
            Method::POST,
            self.endpoint(path)?,
            Some(&AuthBody {
                username,
                password,
                device_info: if cfg!(target_os = "windows") {
                    "noverplay-tui windows"
                } else {
                    "noverplay-tui linux"
                },
                app_version: APP_VERSION,
                captcha_id,
                captcha_answer: &solution.answer,
                captcha_points: &solution.points,
            }),
            None,
        )
        .await
    }

    async fn send<B, T>(
        &self,
        method: Method,
        url: Url,
        body: Option<&B>,
        token: Option<&str>,
    ) -> AccountResult<T>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        let raw_body = body
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| AccountApiError::local("REQUEST_JSON", error.to_string()))?
            .unwrap_or_default();
        let path_query = path_query(&url);
        let signed = self
            .identity
            .sign(method.as_str(), &path_query, &raw_body)
            .map_err(|error| AccountApiError::local("APP_AUTH_SIGN", error.to_string()))?;
        let mut request = self.http.request(method, url);
        for (name, value) in signed.values() {
            request = request.header(name, value);
        }
        if let Some(token) = token.filter(|value| !value.trim().is_empty()) {
            request = request.header(SESSION_HEADER, format!("Bearer {}", token.trim()));
        }
        if body.is_some() {
            request = request
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(raw_body);
        }
        let response = request.send().await.map_err(|error| {
            AccountApiError::local(
                "NETWORK_ERROR",
                format!("Сервер Noverplay не ответил: {error}"),
            )
        })?;
        parse_response(response).await
    }

    fn endpoint(&self, path: &str) -> AccountResult<Url> {
        self.base_url.join(path).map_err(|error| {
            AccountApiError::local("SERVER_URL", format!("Адрес сервера повреждён: {error}"))
        })
    }
}

async fn parse_response<T: DeserializeOwned>(response: Response) -> AccountResult<T> {
    let status = response.status();
    let retry_after_seconds = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    let body = response.text().await.map_err(|error| {
        AccountApiError::local("RESPONSE_READ", format!("Ответ сервера потерялся: {error}"))
    })?;
    if !status.is_success() {
        return Err(server_error(status, retry_after_seconds, &body));
    }
    serde_json::from_str(&body).map_err(|error| {
        AccountApiError::local(
            "RESPONSE_JSON",
            format!("Сервер Noverplay вернул непонятный ответ: {error}"),
        )
    })
}

fn server_error(
    status: StatusCode,
    retry_after_seconds: Option<u64>,
    body: &str,
) -> AccountApiError {
    let parsed = serde_json::from_str::<ErrorEnvelope>(body).ok();
    let error = parsed.and_then(|value| value.error);
    AccountApiError {
        status: Some(status.as_u16()),
        code: error
            .as_ref()
            .and_then(|value| value.code.clone())
            .unwrap_or_else(|| "SERVER_ERROR".to_string()),
        message: error
            .as_ref()
            .and_then(|value| value.message.clone())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| format!("Сервер отклонил запрос ({status})")),
        retry_after_seconds,
    }
}

fn path_query(url: &Url) -> String {
    match url.query() {
        Some(query) => format!("{}?{query}", url.path()),
        None => url.path().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    use super::*;

    #[test]
    fn error_keeps_stable_code_and_retry_after() {
        let error = server_error(
            StatusCode::TOO_MANY_REQUESTS,
            Some(30),
            r#"{"error":{"code":"TUI_LOGIN_RATE_LIMITED","message":"slow down"}}"#,
        );

        assert_eq!(error.code, "TUI_LOGIN_RATE_LIMITED");
        assert_eq!(error.retry_after_seconds, Some(30));
        assert_eq!(error.to_string(), "slow down Повтори через 30 сек");
    }

    #[test]
    fn query_is_signed_exactly_as_it_goes_over_http() {
        let url = Url::parse("https://api.noverplay.space/api/auth/captcha?action=login").unwrap();
        assert_eq!(path_query(&url), "/api/auth/captcha?action=login");
    }

    #[tokio::test]
    async fn captcha_request_carries_tui_signature_headers() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut bytes = [0_u8; 16_384];
            let read = socket.read(&mut bytes).unwrap();
            let request = String::from_utf8_lossy(&bytes[..read]).to_ascii_lowercase();
            assert!(request.starts_with("get /api/auth/captcha?action=login http/1.1"));
            assert!(request.contains("x-noverplay-app-auth: noverplay-app-auth-v2"));
            assert!(request.contains("x-noverplay-client-kind: tui"));
            assert!(request.contains("proverka-na-huesosa:"));
            let body = r#"{"captcha_id":"id","captcha_kind":"icon_sequence","image_data_url":"data:image/svg+xml;base64,AA==","click_count_required":4,"action_type":"login","expires_at":"later","disabled":false}"#;
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
        let client = AccountClient {
            http: Client::builder().build().unwrap(),
            base_url: Url::parse(&format!("http://{address}")).unwrap(),
            identity: InstallationIdentity::load_or_create(&secrets).unwrap(),
        };

        let challenge = client.captcha(AccountAction::Login).await.unwrap();

        server.join().unwrap();
        assert_eq!(challenge.captcha_id, "id");
        assert_eq!(challenge.click_count_required, 4);
    }

    #[tokio::test]
    #[ignore = "ходит на production только при ручном smoke-тесте"]
    async fn live_server_returns_a_question_to_tui() {
        let temp = tempfile::tempdir().unwrap();
        let secrets = SecretStore::file_only(temp.path().join("secrets.json"));
        let server_url = std::env::var("NOVERPLAY_LIVE_SERVER_URL")
            .unwrap_or_else(|_| "https://api.noverplay.space".to_string());
        let client = AccountClient::new(&server_url, &secrets).unwrap();

        let challenge = client.captcha(AccountAction::Login).await.unwrap();

        assert_eq!(challenge.captcha_kind, "text");
        assert!(challenge.image_data_url.is_empty());
        assert!(
            challenge
                .prompt
                .is_some_and(|value| !value.trim().is_empty())
        );
    }
}
