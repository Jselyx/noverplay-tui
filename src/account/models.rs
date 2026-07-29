use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccountAction {
    Login,
    Register,
}

impl AccountAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::Register => "register",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct CaptchaChallenge {
    pub captcha_id: String,
    pub captcha_kind: String,
    pub image_data_url: String,
    pub click_count_required: u8,
    pub action_type: String,
    pub expires_at: String,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub prompt: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct CaptchaPoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CaptchaSolution {
    pub answer: String,
    pub points: Vec<CaptchaPoint>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct AccountUser {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub uid: String,
    pub public_uid: String,
    pub created_at: String,
    #[serde(default)]
    pub avatar_url: String,
    #[serde(default)]
    pub telemetry_opt_in: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LoginResponse {
    pub user: AccountUser,
    pub session_token: String,
    pub expires_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountSession {
    pub user: AccountUser,
    pub expires_at: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SessionResponse {
    pub user: AccountUser,
    pub expires_at: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BootstrapResponse {
    pub protocol: String,
    pub client_kind: String,
    pub refresh_after_seconds: i64,
    pub refresh_at: String,
    pub soundcloud: SoundCloudBootstrap,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SoundCloudBootstrap {
    pub client_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BootstrapUpdate {
    pub protocol: String,
    pub refresh_at: String,
    pub refresh_at_ms: i64,
}

#[derive(Serialize)]
pub(super) struct AuthBody<'a> {
    pub username: &'a str,
    pub password: &'a str,
    pub device_info: &'static str,
    pub app_version: &'static str,
    pub captcha_id: &'a str,
    pub captcha_answer: &'a str,
    pub captcha_points: &'a [CaptchaPoint],
}

#[derive(Debug, Deserialize)]
pub(super) struct ErrorEnvelope {
    pub error: Option<ErrorBody>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ErrorBody {
    pub code: Option<String>,
    pub message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_user_accepts_extra_cosmetics_without_dragging_them_into_state() {
        let user: AccountUser = serde_json::from_str(
            r#"{"id":"1","username":"user","display_name":"User","uid":"42","public_uid":"pub","created_at":"now","cosmetics":{"hat":true}}"#,
        )
        .unwrap();

        assert_eq!(user.display_name, "User");
        assert!(user.avatar_url.is_empty());
    }
}
