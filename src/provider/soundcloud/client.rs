use anyhow::{Context, Result, bail};
use reqwest::{Client, ClientBuilder};
use serde::de::DeserializeOwned;
use url::Url;

#[derive(Clone)]
pub(super) struct SoundCloudClient {
    http: Client,
    client_id: String,
    api_v2: Url,
}

impl SoundCloudClient {
    pub(super) fn new(client_id: String) -> Result<Self> {
        Self::from_parts(
            client_id,
            Url::parse("https://api-v2.soundcloud.com/")?,
            build_http(Client::builder())?,
        )
    }

    #[cfg(test)]
    pub(super) fn with_base(client_id: String, api_v2: Url) -> Result<Self> {
        // Пул увидел полуживой мок-сокет и решил устроить лотерею, в тестах этот балаган закрыт
        let http = build_http(Client::builder().pool_max_idle_per_host(0))?;
        Self::from_parts(client_id, api_v2, http)
    }

    fn from_parts(client_id: String, api_v2: Url, http: Client) -> Result<Self> {
        if client_id.trim().is_empty() {
            bail!("нужен client_id SoundCloud")
        }
        Ok(Self {
            http,
            client_id,
            api_v2,
        })
    }

    pub(super) fn v2_url(&self, path: &[&str]) -> Result<Url> {
        append_path(self.api_v2.clone(), path)
    }

    pub(super) async fn get_json<T>(&self, url: Url, query: &[(&str, String)]) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let response = self
            .http
            .get(url)
            .query(query)
            .query(&[("client_id", self.client_id.as_str())])
            .send()
            .await
            .context("SoundCloud не ответил")?
            .error_for_status()
            .context("SoundCloud отклонил запрос")?;
        response
            .json()
            .await
            .context("SoundCloud вернул непонятный JSON")
    }
}

fn build_http(builder: ClientBuilder) -> Result<Client> {
    builder
        .user_agent(format!("noverplay-tui/{}", crate::APP_VERSION))
        .build()
        .context("не удалось создать HTTP-клиент SoundCloud")
}

fn append_path(mut base: Url, path: &[&str]) -> Result<Url> {
    base.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("адрес SoundCloud нельзя изменить"))?
        .extend(path);
    Ok(base)
}
