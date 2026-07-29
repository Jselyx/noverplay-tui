use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountApiError {
    pub status: Option<u16>,
    pub code: String,
    pub message: String,
    pub retry_after_seconds: Option<u64>,
}

impl AccountApiError {
    pub fn local(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status: None,
            code: code.into(),
            message: message.into(),
            retry_after_seconds: None,
        }
    }

    pub fn captcha_expired(&self) -> bool {
        matches!(self.code.as_str(), "CAPTCHA_EXPIRED" | "CAPTCHA_REQUIRED")
    }
}

impl fmt::Display for AccountApiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(seconds) = self.retry_after_seconds {
            write!(formatter, "{} Повтори через {seconds} сек", self.message)
        } else {
            formatter.write_str(&self.message)
        }
    }
}

impl std::error::Error for AccountApiError {}

pub type AccountResult<T> = Result<T, AccountApiError>;
