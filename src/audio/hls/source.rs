use std::{
    collections::BTreeMap,
    io::{self, Read, Seek, SeekFrom},
};

use anyhow::{Context, Result, ensure};
use symphonia::core::io::MediaSource;
use url::Url;

use super::{
    client::HlsClient,
    manifest::{HlsAsset, HlsMediaPlaylist, HlsPlaylist, parse_playlist},
    sniff::sniff_extension,
};

const MAX_PLAYLIST_DEPTH: usize = 3;

pub(in crate::audio) struct HlsSource {
    client: HlsClient,
    playlist: HlsMediaPlaylist,
    next_segment: usize,
    waiting_segment: Option<usize>,
    loaded_init: Option<HlsAsset>,
    current: Vec<u8>,
    current_offset: usize,
    position: u64,
    extension: &'static str,
    start_ms: u64,
}

impl HlsSource {
    pub(in crate::audio) fn open(
        url: &Url,
        headers: &BTreeMap<String, String>,
        position_ms: u64,
    ) -> Result<Self> {
        let client = HlsClient::new(headers)?;
        let playlist = load_media_playlist(&client, url)?;
        let (next_segment, start_ms) = segment_for_position(&playlist, position_ms);
        let mut source = Self {
            client,
            playlist,
            next_segment,
            waiting_segment: None,
            loaded_init: None,
            current: Vec::new(),
            current_offset: 0,
            position: 0,
            extension: "",
            start_ms,
        };
        ensure!(source.load_next()?, "в HLS-потоке нет данных");
        let asset = source
            .current_asset()
            .context("у HLS пропал первый ресурс")?;
        source.extension = sniff_extension(&source.current, asset)?;
        Ok(source)
    }

    pub(in crate::audio) const fn extension(&self) -> &'static str {
        self.extension
    }

    pub(in crate::audio) const fn start_ms(&self) -> u64 {
        self.start_ms
    }

    fn load_next(&mut self) -> Result<bool> {
        if let Some(index) = self.waiting_segment.take() {
            self.load_segment(index)?;
            return Ok(true);
        }
        let Some(segment) = self.playlist.segments.get(self.next_segment).cloned() else {
            return Ok(false);
        };
        if segment.init != self.loaded_init {
            self.loaded_init = segment.init.clone();
            if let Some(init) = segment.init {
                self.current = self.client.fetch_asset(&init)?;
                self.current_offset = 0;
                self.waiting_segment = Some(self.next_segment);
                return Ok(true);
            }
        }
        self.load_segment(self.next_segment)?;
        Ok(true)
    }

    fn load_segment(&mut self, index: usize) -> Result<()> {
        let segment = self
            .playlist
            .segments
            .get(index)
            .context("HLS потерял сегмент во время чтения")?;
        self.current = self.client.fetch_asset(&segment.asset)?;
        self.current_offset = 0;
        self.next_segment = index + 1;
        Ok(())
    }

    fn current_asset(&self) -> Option<&HlsAsset> {
        if self.waiting_segment.is_some() {
            self.loaded_init.as_ref()
        } else {
            self.playlist
                .segments
                .get(self.next_segment.saturating_sub(1))
                .map(|segment| &segment.asset)
        }
    }
}

impl Read for HlsSource {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        while self.current_offset >= self.current.len() {
            if !self.load_next().map_err(io_error)? {
                return Ok(0);
            }
        }
        let count = output
            .len()
            .min(self.current.len().saturating_sub(self.current_offset));
        output[..count]
            .copy_from_slice(&self.current[self.current_offset..self.current_offset + count]);
        self.current_offset += count;
        self.position += count as u64;
        Ok(count)
    }
}

impl Seek for HlsSource {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let unchanged = matches!(from, SeekFrom::Current(0))
            || matches!(from, SeekFrom::Start(position) if position == self.position);
        if unchanged {
            Ok(self.position)
        } else {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "HLS перематывается по времени, а не байтами",
            ))
        }
    }
}

impl MediaSource for HlsSource {
    fn is_seekable(&self) -> bool {
        false
    }

    fn byte_len(&self) -> Option<u64> {
        None
    }
}

fn load_media_playlist(client: &HlsClient, initial_url: &Url) -> Result<HlsMediaPlaylist> {
    let mut url = initial_url.clone();
    for _ in 0..MAX_PLAYLIST_DEPTH {
        let body = client.fetch_manifest(&url)?;
        match parse_playlist(&url, &body)? {
            HlsPlaylist::Media(playlist) => return Ok(playlist),
            HlsPlaylist::Master(variants) => {
                // Качество выбираем числом, шаман с ушами за триста баксов сегодня выходной
                url = variants
                    .into_iter()
                    .max_by_key(|variant| variant.bandwidth)
                    .context("в master playlist нет вариантов")?
                    .url;
            }
        }
    }
    anyhow::bail!("слишком глубокая цепочка HLS-плейлистов")
}

fn segment_for_position(playlist: &HlsMediaPlaylist, position_ms: u64) -> (usize, u64) {
    let mut elapsed_ms = 0u64;
    for (index, segment) in playlist.segments.iter().enumerate() {
        let end_ms = elapsed_ms.saturating_add(segment.duration_ms);
        if position_ms < end_ms || index + 1 == playlist.segments.len() {
            return (index, elapsed_ms);
        }
        elapsed_ms = end_ms;
    }
    (0, 0)
}

fn io_error(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}
