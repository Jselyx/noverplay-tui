use anyhow::{Context, Result, bail};
use yandex_music::YandexMusicClient;

pub(super) fn build_client(token: &str) -> Result<YandexMusicClient> {
    let token = normalizovat_token(token);
    if token.is_empty() {
        bail!("нужен OAuth-токен Yandex Music")
    }
    YandexMusicClient::builder(token)
        .build()
        .context("не удалось создать клиент Yandex Music")
}

fn normalizovat_token(token: &str) -> &str {
    token
        .trim()
        .strip_prefix("OAuth ")
        .unwrap_or_else(|| token.trim())
        .trim()
}

#[cfg(test)]
mod tests {
    use super::normalizovat_token;

    #[test]
    fn oauth_prefix_is_not_stored_twice() {
        assert_eq!(normalizovat_token(" OAuth token-value "), "token-value");
        assert_eq!(normalizovat_token("token-value"), "token-value");
    }
}
