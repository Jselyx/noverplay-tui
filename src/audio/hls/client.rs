use std::{collections::BTreeMap, io::Read, thread, time::Duration};

use anyhow::{Context, Result, bail, ensure};
use reqwest::{
    StatusCode,
    blocking::{Client, Response},
    header::{CONTENT_LENGTH, HeaderMap, HeaderName, HeaderValue, RANGE},
    redirect::Policy,
};
use url::Url;

use super::manifest::{HlsAsset, validate_url};

const MANIFEST_LIMIT: usize = 1024 * 1024;
const SEGMENT_LIMIT: usize = 8 * 1024 * 1024;

pub(super) struct HlsClient {
    client: Client,
    headers: HeaderMap,
}

impl HlsClient {
    pub(super) fn new(headers: &BTreeMap<String, String>) -> Result<Self> {
        let headers = parse_headers(headers)?;
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .redirect(Policy::custom(|attempt| {
                if attempt.previous().len() >= 5 || validate_url(attempt.url()).is_err() {
                    attempt.stop()
                } else {
                    attempt.follow()
                }
            }))
            .build()?;
        Ok(Self { client, headers })
    }

    pub(super) fn fetch_manifest(&self, url: &Url) -> Result<String> {
        validate_url(url)?;
        let response = self.send_with_retry(url, None)?;
        let bytes = read_limited(response, MANIFEST_LIMIT, None)?;
        String::from_utf8(bytes).context("HLS-плейлист пришёл не в UTF-8")
    }

    pub(super) fn fetch_asset(&self, asset: &HlsAsset) -> Result<Vec<u8>> {
        validate_url(&asset.url)?;
        if let Some(byte_range) = &asset.byte_range {
            ensure!(
                byte_range.length <= SEGMENT_LIMIT as u64,
                "HLS-сегмент подозрительно жирный"
            );
        }
        let response = self.send_with_retry(&asset.url, asset.byte_range.as_ref())?;
        read_limited(
            response,
            SEGMENT_LIMIT,
            asset.byte_range.as_ref().map(|range| range.length),
        )
    }

    fn send_with_retry(
        &self,
        url: &Url,
        byte_range: Option<&super::manifest::HlsByteRange>,
    ) -> Result<Response> {
        let mut last_error = None;
        for attempt in 0..3 {
            match self.send(url, byte_range) {
                Ok(response) => return Ok(response),
                Err(error) => {
                    last_error = Some(error);
                    if attempt < 2 {
                        // CDN иногда изображает кирпич, ждём чутка и проверяем не закончился ли спектакль
                        thread::sleep(Duration::from_millis(150 * (attempt + 1)));
                    }
                }
            }
        }
        Err(last_error.context("HLS-источник не ответил")?)
    }

    fn send(
        &self,
        url: &Url,
        byte_range: Option<&super::manifest::HlsByteRange>,
    ) -> Result<Response> {
        let mut request = self.client.get(url.clone()).headers(self.headers.clone());
        if let Some(byte_range) = byte_range {
            let end = byte_range
                .start
                .checked_add(byte_range.length - 1)
                .context("HLS byte range переполнился")?;
            request = request.header(RANGE, format!("bytes={}-{end}", byte_range.start));
        }
        let response = request.send().context("не удалось запросить HLS-ресурс")?;
        ensure!(
            response.status().is_success(),
            "HLS-ресурс вернул HTTP {}",
            response.status()
        );
        if byte_range.is_some() && response.status() != StatusCode::PARTIAL_CONTENT {
            bail!("HLS-сервер проигнорировал byte range")
        }
        Ok(response)
    }
}

fn read_limited(mut response: Response, limit: usize, exact: Option<u64>) -> Result<Vec<u8>> {
    if let Some(length) = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
    {
        ensure!(
            length <= limit as u64,
            "HLS-ресурс больше разрешённого размера"
        );
    }
    let mut bytes = Vec::with_capacity(exact.unwrap_or(64 * 1024) as usize);
    response
        .by_ref()
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .context("не удалось прочитать HLS-ресурс")?;
    ensure!(
        bytes.len() <= limit,
        "HLS-ресурс больше разрешённого размера"
    );
    if let Some(exact) = exact {
        ensure!(bytes.len() as u64 == exact, "HLS-сервер обрезал byte range");
    }
    Ok(bytes)
}

fn parse_headers(headers: &BTreeMap<String, String>) -> Result<HeaderMap> {
    let mut parsed = HeaderMap::new();
    for (name, value) in headers {
        parsed.insert(
            HeaderName::from_bytes(name.as_bytes()).context("неверное имя HTTP-заголовка")?,
            HeaderValue::from_str(value).context("неверное значение HTTP-заголовка")?,
        );
    }
    Ok(parsed)
}
