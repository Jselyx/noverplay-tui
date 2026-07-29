use anyhow::{Result, bail};
use zeroize::Zeroize;

use super::{
    captcha::{CaptchaRaster, captcha_cell_area, click_to_point},
    models::{AccountAction, AccountUser, CaptchaChallenge, CaptchaPoint, CaptchaSolution},
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum AccountState {
    #[default]
    Guest,
    Restoring,
    Authenticated {
        user: AccountUser,
        expires_at: String,
    },
}

impl AccountState {
    pub fn user(&self) -> Option<&AccountUser> {
        match self {
            Self::Authenticated { user, .. } => Some(user),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AccountDialogStage {
    #[default]
    Credentials,
    LoadingCaptcha,
    Captcha,
    Submitting,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CaptchaClick {
    x: u16,
    y: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountDialog {
    pub action: AccountAction,
    pub stage: AccountDialogStage,
    pub selected_field: usize,
    pub username: String,
    pub password: String,
    pub challenge: Option<CaptchaChallenge>,
    pub raster: Option<CaptchaRaster>,
    pub captcha_answer: String,
    clicks: Vec<CaptchaClick>,
    pub error: Option<String>,
}

impl AccountDialog {
    pub fn new(action: AccountAction) -> Self {
        Self {
            action,
            stage: AccountDialogStage::Credentials,
            selected_field: 0,
            username: String::new(),
            password: String::new(),
            challenge: None,
            raster: None,
            captcha_answer: String::new(),
            clicks: Vec::new(),
            error: None,
        }
    }

    pub fn input(&mut self, value: char) {
        if value.is_control() {
            return;
        }
        match self.stage {
            AccountDialogStage::Credentials if self.selected_field == 0 => {
                if self.username.len() < 32 && value.is_ascii_alphanumeric() {
                    self.username.push(value);
                }
            }
            AccountDialogStage::Credentials => {
                if self.password.chars().count() < 128 {
                    self.password.push(value);
                }
            }
            AccountDialogStage::Captcha if self.is_text_captcha() => {
                if self.captcha_answer.len() < 16 && value.is_ascii_alphanumeric() {
                    self.captcha_answer.push(value.to_ascii_uppercase());
                }
            }
            _ => {}
        }
    }

    pub fn backspace(&mut self) {
        match self.stage {
            AccountDialogStage::Credentials if self.selected_field == 0 => {
                self.username.pop();
            }
            AccountDialogStage::Credentials => {
                self.password.pop();
            }
            AccountDialogStage::Captcha if self.is_text_captcha() => {
                self.captcha_answer.pop();
            }
            AccountDialogStage::Captcha => {
                self.clicks.pop();
            }
            _ => {}
        }
    }

    pub fn select_next(&mut self) {
        if self.stage == AccountDialogStage::Credentials {
            self.selected_field = (self.selected_field + 1) % 2;
        }
    }

    pub fn select_previous(&mut self) {
        if self.stage == AccountDialogStage::Credentials {
            self.selected_field = (self.selected_field + 1) % 2;
        }
    }

    pub fn toggle_action(&mut self) {
        if self.stage != AccountDialogStage::Credentials {
            return;
        }
        self.action = match self.action {
            AccountAction::Login => AccountAction::Register,
            AccountAction::Register => AccountAction::Login,
        };
        self.error = None;
    }

    pub fn validate_credentials(&self) -> Result<()> {
        if !(3..=32).contains(&self.username.len())
            || !self
                .username
                .chars()
                .all(|value| value.is_ascii_alphanumeric())
        {
            bail!("Логин: 3-32 английские буквы или цифры")
        }
        if !(8..=128).contains(&self.password.chars().count()) {
            bail!("Пароль: 8-128 символов")
        }
        Ok(())
    }

    pub fn set_challenge(&mut self, challenge: CaptchaChallenge) -> Result<()> {
        let raster = if challenge.captcha_kind == "text" {
            None
        } else {
            Some(CaptchaRaster::from_data_url(&challenge.image_data_url)?)
        };
        self.challenge = Some(challenge);
        self.raster = raster;
        self.captcha_answer.clear();
        self.clicks.clear();
        self.stage = AccountDialogStage::Captcha;
        self.error = None;
        Ok(())
    }

    pub fn click(&mut self, column: u16, row: u16, terminal_width: u16, terminal_height: u16) {
        if self.stage != AccountDialogStage::Captcha || self.is_text_captcha() {
            return;
        }
        let required = self.required_clicks();
        if self.clicks.len() >= required {
            return;
        }
        let area = captcha_cell_area(terminal_width, terminal_height);
        if let Some(point) = click_to_point(area, column, row) {
            self.clicks.push(CaptchaClick {
                x: (point.x * 10_000.0).round().clamp(0.0, 10_000.0) as u16,
                y: (point.y * 10_000.0).round().clamp(0.0, 10_000.0) as u16,
            });
        }
    }

    pub fn selected_clicks(&self) -> usize {
        self.clicks.len()
    }

    pub fn required_clicks(&self) -> usize {
        self.challenge
            .as_ref()
            .map(|challenge| usize::from(challenge.click_count_required))
            .unwrap_or_default()
    }

    pub fn solution(&self) -> Result<CaptchaSolution> {
        if self.is_text_captcha() {
            if self.captcha_answer.trim().is_empty() {
                bail!("Введи ответ на задачу")
            }
        } else if self.clicks.len() != self.required_clicks() || self.clicks.is_empty() {
            bail!("Нажми иконки в указанном порядке")
        }
        Ok(CaptchaSolution {
            answer: self.captcha_answer.clone(),
            points: self
                .clicks
                .iter()
                .map(|point| CaptchaPoint {
                    x: f64::from(point.x) / 10_000.0,
                    y: f64::from(point.y) / 10_000.0,
                })
                .collect(),
        })
    }

    pub fn captcha_id(&self) -> &str {
        self.challenge
            .as_ref()
            .map(|challenge| challenge.captcha_id.as_str())
            .unwrap_or_default()
    }

    pub fn authentication_failed(&mut self, message: String) {
        self.stage = AccountDialogStage::Captcha;
        self.clicks.clear();
        self.captcha_answer.clear();
        self.error = Some(message);
    }

    fn is_text_captcha(&self) -> bool {
        self.challenge
            .as_ref()
            .is_some_and(|challenge| challenge.captcha_kind == "text")
    }
}

impl Drop for AccountDialog {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use base64::{Engine, engine::general_purpose::STANDARD};

    use super::*;

    #[test]
    fn credentials_follow_the_same_rules_as_server() {
        let mut dialog = AccountDialog::new(AccountAction::Login);
        for value in "User123".chars() {
            dialog.input(value);
        }
        dialog.select_next();
        for value in "password123".chars() {
            dialog.input(value);
        }

        assert!(dialog.validate_credentials().is_ok());
        assert_eq!(dialog.username, "User123");
    }

    #[test]
    fn four_human_clicks_turn_into_normalized_server_points() {
        let mut dialog = AccountDialog::new(AccountAction::Login);
        dialog.set_challenge(challenge()).unwrap();
        let area = captcha_cell_area(100, 32);
        for offset in 1..=4 {
            dialog.click(area.x + offset, area.y + 2, 100, 32);
        }

        let solution = dialog.solution().unwrap();
        assert_eq!(solution.points.len(), 4);
        assert!(
            solution
                .points
                .iter()
                .all(|point| (0.0..=1.0).contains(&point.x))
        );
    }

    fn challenge() -> CaptchaChallenge {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="360" height="236"><rect width="360" height="236" fill="#ffffff"/></svg>"##;
        CaptchaChallenge {
            captcha_id: "captcha".to_string(),
            captcha_kind: "icon_sequence".to_string(),
            image_data_url: format!("data:image/svg+xml;base64,{}", STANDARD.encode(svg)),
            click_count_required: 4,
            action_type: "login".to_string(),
            expires_at: "later".to_string(),
            disabled: false,
            prompt: None,
        }
    }

    #[test]
    fn text_challenge_does_not_demand_a_fake_picture() {
        let mut dialog = AccountDialog::new(AccountAction::Login);
        let mut challenge = challenge();
        challenge.captcha_kind = "text".to_string();
        challenge.image_data_url.clear();
        challenge.prompt = Some("Сколько будет 2 + 2?".to_string());
        dialog.set_challenge(challenge).unwrap();

        assert!(dialog.raster.is_none());
        dialog.input('4');
        assert_eq!(dialog.solution().unwrap().answer, "4");
    }
}
