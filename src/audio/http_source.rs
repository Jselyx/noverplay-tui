use std::{
    collections::BTreeMap,
    io::{self, Read, Seek, SeekFrom},
    thread,
    time::Duration,
};

use anyhow::{Context, Result};
use reqwest::{
    StatusCode,
    blocking::Client,
    header::{CONTENT_LENGTH, CONTENT_RANGE, HeaderMap, HeaderName, HeaderValue, RANGE},
};
use symphonia::core::io::MediaSource;

const HTTP_CHUNK_BYTES: usize = 256 * 1024;

pub(super) struct HttpRangeSource {
    client: Client,
    url: String,
    headers: HeaderMap,
    position: u64,
    length: Option<u64>,
    range_supported: bool,
    chunk_start: u64,
    chunk: Vec<u8>,
}

impl HttpRangeSource {
    pub(super) fn open(
        url: &str,
        headers: &BTreeMap<String, String>,
        prefer_range: bool,
    ) -> Result<Self> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()?;
        let headers = normalize_headers(headers)?;
        let mut source = Self {
            client,
            url: url.to_string(),
            headers,
            position: 0,
            length: None,
            range_supported: prefer_range,
            chunk_start: 0,
            chunk: Vec::new(),
        };
        source.fetch(0)?;
        Ok(source)
    }

    fn fetch(&mut self, start: u64) -> io::Result<()> {
        let mut last_error = None;
        for attempt in 0..3 {
            match self.fetch_once(start) {
                Ok(()) => return Ok(()),
                Err(error) => {
                    last_error = Some(error);
                    if attempt < 2 {
                        // Сеть прилегла на шконку, даём ей три шанса встать и не позориться 🫩
                        thread::sleep(Duration::from_millis(150 * (attempt + 1)));
                    }
                }
            }
        }
        Err(last_error.unwrap_or_else(|| io::Error::other("источник не ответил")))
    }

    fn fetch_once(&mut self, start: u64) -> io::Result<()> {
        let end = start.saturating_add(HTTP_CHUNK_BYTES as u64 - 1);
        let mut request = self.client.get(&self.url).headers(self.headers.clone());
        if self.range_supported || start > 0 {
            request = request.header(RANGE, format!("bytes={start}-{end}"));
        }
        let response = request.send().map_err(io_error)?;
        let status = response.status();
        if !status.is_success() {
            return Err(io::Error::other(format!("источник вернул HTTP {status}")));
        }
        let headers = response.headers().clone();
        let partial = status == StatusCode::PARTIAL_CONTENT;
        if start > 0 && !partial {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "сервер игнорирует HTTP Range",
            ));
        }
        self.range_supported = partial;
        self.length = dlina_iz_range(&headers).or_else(|| {
            if start == 0 && !partial {
                headers
                    .get(CONTENT_LENGTH)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse().ok())
            } else {
                self.length
            }
        });
        let mut chunk = Vec::with_capacity(HTTP_CHUNK_BYTES);
        response
            .take(HTTP_CHUNK_BYTES as u64)
            .read_to_end(&mut chunk)
            .map_err(io_error)?;
        self.chunk_start = start;
        self.chunk = chunk;
        Ok(())
    }
}

impl Read for HttpRangeSource {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() || self.length.is_some_and(|length| self.position >= length) {
            return Ok(0);
        }
        let chunk_end = self.chunk_start + self.chunk.len() as u64;
        if self.position < self.chunk_start || self.position >= chunk_end {
            self.fetch(self.position)?;
            if self.chunk.is_empty() {
                return Ok(0);
            }
        }
        let offset = (self.position - self.chunk_start) as usize;
        let count = output.len().min(self.chunk.len() - offset);
        output[..count].copy_from_slice(&self.chunk[offset..offset + count]);
        self.position += count as u64;
        Ok(count)
    }
}

impl Seek for HttpRangeSource {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let target = match from {
            SeekFrom::Start(position) => position as i128,
            SeekFrom::Current(offset) => self.position as i128 + offset as i128,
            SeekFrom::End(offset) => {
                self.length
                    .ok_or_else(|| io::Error::new(io::ErrorKind::Unsupported, "длина неизвестна"))?
                    as i128
                    + offset as i128
            }
        };
        if target < 0 || target > u64::MAX as i128 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "позиция вне источника",
            ));
        }
        let target = target as u64;
        if !self.range_supported && target != self.position {
            // Сервер сказал Range? НЕ ЗНАЮ ТАКОГО, сиди теперь без перемотки как честный арестант ✌️
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "источник не поддерживает HTTP Range",
            ));
        }
        self.position = target;
        Ok(target)
    }
}

impl MediaSource for HttpRangeSource {
    fn is_seekable(&self) -> bool {
        self.range_supported
    }

    fn byte_len(&self) -> Option<u64> {
        self.length
    }
}

fn normalize_headers(headers: &BTreeMap<String, String>) -> Result<HeaderMap> {
    let mut parsed = HeaderMap::new();
    for (name, value) in headers {
        parsed.insert(
            HeaderName::from_bytes(name.as_bytes()).context("неверное имя HTTP-заголовка")?,
            HeaderValue::from_str(value).context("неверное значение HTTP-заголовка")?,
        );
    }
    Ok(parsed)
}

pub(super) fn dlina_iz_range(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(CONTENT_RANGE)?
        .to_str()
        .ok()?
        .rsplit_once('/')?
        .1
        .parse()
        .ok()
}

fn io_error(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}
