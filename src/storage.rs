use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::model::{Playlist, RepeatMode, TrackRef};

#[derive(Clone, Debug)]
pub struct Storage {
    path: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryEntry {
    pub track: TrackRef,
    pub played_at_ms: i64,
    pub completed: bool,
    pub skipped: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct QueueSnapshot {
    pub tracks: Vec<TrackRef>,
    pub current_index: Option<usize>,
    pub shuffle: bool,
    pub repeat: RepeatMode,
}

impl Storage {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn initialize(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Не удалось создать каталог БД {}", parent.display()))?;
        }
        let connection = self.connection()?;
        connection.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS schema_meta (
                version INTEGER NOT NULL
            );
            INSERT INTO schema_meta(version)
            SELECT 1 WHERE NOT EXISTS (SELECT 1 FROM schema_meta);

            CREATE TABLE IF NOT EXISTS playlists (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                description TEXT NOT NULL,
                source_url TEXT,
                created_at_ms INTEGER NOT NULL,
                updated_at_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS playlist_tracks (
                playlist_id TEXT NOT NULL,
                position INTEGER NOT NULL,
                provider_key TEXT NOT NULL,
                track_json TEXT NOT NULL,
                PRIMARY KEY (playlist_id, position),
                FOREIGN KEY (playlist_id) REFERENCES playlists(id) ON DELETE CASCADE
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_playlist_track_provider
                ON playlist_tracks(playlist_id, provider_key);

            CREATE TABLE IF NOT EXISTS library_tracks (
                provider_key TEXT PRIMARY KEY,
                canonical_key TEXT NOT NULL,
                track_json TEXT NOT NULL,
                liked_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_library_canonical
                ON library_tracks(canonical_key);

            CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                provider_key TEXT NOT NULL,
                track_json TEXT NOT NULL,
                played_at_ms INTEGER NOT NULL,
                completed INTEGER NOT NULL,
                skipped INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_history_played
                ON history(played_at_ms DESC);

            CREATE TABLE IF NOT EXISTS queue_tracks (
                position INTEGER PRIMARY KEY,
                track_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS queue_state (
                singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                current_index INTEGER,
                shuffle INTEGER NOT NULL,
                repeat_mode TEXT NOT NULL
            );
            ",
        )?;
        Ok(())
    }

    pub fn save_playlist(&self, playlist: &Playlist) -> Result<()> {
        // один плейлист одна транзакция, потому что половина плейлиста это уже современное искусство 🤡
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "
            INSERT INTO playlists(id, title, description, source_url, created_at_ms, updated_at_ms)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                description = excluded.description,
                source_url = excluded.source_url,
                updated_at_ms = excluded.updated_at_ms
            ",
            params![
                playlist.id.to_string(),
                playlist.title,
                playlist.description,
                playlist.source_url.as_ref().map(ToString::to_string),
                playlist.created_at_ms,
                playlist.updated_at_ms,
            ],
        )?;
        transaction.execute(
            "DELETE FROM playlist_tracks WHERE playlist_id = ?1",
            [playlist.id.to_string()],
        )?;
        for (position, track) in playlist.tracks.iter().enumerate() {
            transaction.execute(
                "
                INSERT INTO playlist_tracks(playlist_id, position, provider_key, track_json)
                VALUES (?1, ?2, ?3, ?4)
                ",
                params![
                    playlist.id.to_string(),
                    position as i64,
                    track.provider_key(),
                    encode_track(track)?,
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn delete_playlist(&self, id: Uuid) -> Result<bool> {
        let changed = self
            .connection()?
            .execute("DELETE FROM playlists WHERE id = ?1", [id.to_string()])?;
        Ok(changed > 0)
    }

    pub fn list_playlists(&self) -> Result<Vec<Playlist>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT id, title, description, source_url, created_at_ms, updated_at_ms
            FROM playlists
            ORDER BY updated_at_ms DESC, title COLLATE NOCASE
            ",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })?;
        let mut playlists = Vec::new();
        for row in rows {
            let (id, title, description, source_url, created_at_ms, updated_at_ms) = row?;
            let id = Uuid::parse_str(&id).context("В БД найден повреждённый плейлист")?;
            let source_url = source_url
                .map(|value| url::Url::parse(&value))
                .transpose()
                .context("В БД найден повреждённый URL плейлиста")?;
            let tracks = load_playlist_tracks(&connection, id)?;
            playlists.push(Playlist {
                id,
                title,
                description,
                source_url,
                tracks,
                created_at_ms,
                updated_at_ms,
            });
        }
        Ok(playlists)
    }

    pub fn like_track(&self, track: &TrackRef, liked_at_ms: i64) -> Result<()> {
        self.connection()?.execute(
            "
            INSERT INTO library_tracks(provider_key, canonical_key, track_json, liked_at_ms)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(provider_key) DO UPDATE SET
                canonical_key = excluded.canonical_key,
                track_json = excluded.track_json,
                liked_at_ms = excluded.liked_at_ms
            ",
            params![
                track.provider_key(),
                track.canonical_key(),
                encode_track(track)?,
                liked_at_ms,
            ],
        )?;
        Ok(())
    }

    pub fn unlike_track(&self, track: &TrackRef) -> Result<bool> {
        let changed = self.connection()?.execute(
            "DELETE FROM library_tracks WHERE provider_key = ?1",
            [track.provider_key()],
        )?;
        Ok(changed > 0)
    }

    pub fn library_tracks(&self) -> Result<Vec<TrackRef>> {
        load_track_column(
            &self.connection()?,
            "SELECT track_json FROM library_tracks ORDER BY liked_at_ms DESC",
            &[],
        )
    }

    pub fn liked_tracks_with_time(&self) -> Result<Vec<(TrackRef, i64)>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT track_json, liked_at_ms FROM library_tracks ORDER BY liked_at_ms DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut tracks = Vec::new();
        for row in rows {
            let (track, liked_at_ms) = row?;
            tracks.push((decode_track(&track)?, liked_at_ms));
        }
        Ok(tracks)
    }

    pub fn record_history(&self, entry: &HistoryEntry) -> Result<()> {
        self.connection()?.execute(
            "
            INSERT INTO history(provider_key, track_json, played_at_ms, completed, skipped)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ",
            params![
                entry.track.provider_key(),
                encode_track(&entry.track)?,
                entry.played_at_ms,
                entry.completed,
                entry.skipped,
            ],
        )?;
        Ok(())
    }

    pub fn recent_history(&self, limit: usize) -> Result<Vec<HistoryEntry>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT track_json, played_at_ms, completed, skipped
            FROM history
            ORDER BY played_at_ms DESC
            LIMIT ?1
            ",
        )?;
        let rows = statement.query_map([limit.clamp(1, 10_000) as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, bool>(2)?,
                row.get::<_, bool>(3)?,
            ))
        })?;
        let mut history = Vec::new();
        for row in rows {
            let (track, played_at_ms, completed, skipped) = row?;
            history.push(HistoryEntry {
                track: decode_track(&track)?,
                played_at_ms,
                completed,
                skipped,
            });
        }
        Ok(history)
    }

    pub fn history_between(
        &self,
        start_ms: i64,
        end_ms: i64,
        limit: usize,
    ) -> Result<Vec<HistoryEntry>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT track_json, played_at_ms, completed, skipped
            FROM history
            WHERE played_at_ms >= ?1 AND played_at_ms < ?2
            ORDER BY played_at_ms DESC
            LIMIT ?3
            ",
        )?;
        let rows = statement.query_map(
            params![start_ms, end_ms, limit.clamp(1, 10_000) as i64],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, bool>(2)?,
                    row.get::<_, bool>(3)?,
                ))
            },
        )?;
        let mut history = Vec::new();
        for row in rows {
            let (track, played_at_ms, completed, skipped) = row?;
            history.push(HistoryEntry {
                track: decode_track(&track)?,
                played_at_ms,
                completed,
                skipped,
            });
        }
        Ok(history)
    }

    pub fn save_queue(&self, snapshot: &QueueSnapshot) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM queue_tracks", [])?;
        for (position, track) in snapshot.tracks.iter().enumerate() {
            transaction.execute(
                "INSERT INTO queue_tracks(position, track_json) VALUES (?1, ?2)",
                params![position as i64, encode_track(track)?],
            )?;
        }
        transaction.execute(
            "
            INSERT INTO queue_state(singleton, current_index, shuffle, repeat_mode)
            VALUES (1, ?1, ?2, ?3)
            ON CONFLICT(singleton) DO UPDATE SET
                current_index = excluded.current_index,
                shuffle = excluded.shuffle,
                repeat_mode = excluded.repeat_mode
            ",
            params![
                snapshot.current_index.map(|value| value as i64),
                snapshot.shuffle,
                repeat_name(snapshot.repeat),
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn load_queue(&self) -> Result<QueueSnapshot> {
        let connection = self.connection()?;
        let tracks = load_track_column(
            &connection,
            "SELECT track_json FROM queue_tracks ORDER BY position",
            &[],
        )?;
        let state = connection
            .query_row(
                "SELECT current_index, shuffle, repeat_mode FROM queue_state WHERE singleton = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, Option<i64>>(0)?,
                        row.get::<_, bool>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;
        let Some((current_index, shuffle, repeat)) = state else {
            return Ok(QueueSnapshot::default());
        };
        let current_index = current_index
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value < tracks.len());
        Ok(QueueSnapshot {
            tracks,
            current_index,
            shuffle,
            repeat: parse_repeat(&repeat),
        })
    }

    fn connection(&self) -> Result<Connection> {
        let connection = Connection::open(&self.path)
            .with_context(|| format!("Не удалось открыть БД {}", self.path.display()))?;
        // WAL, потому что морозить плеер одной записью умеют и без нас АХАХАХА 🫩
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        Ok(connection)
    }
}

fn load_playlist_tracks(connection: &Connection, id: Uuid) -> Result<Vec<TrackRef>> {
    load_track_column(
        connection,
        "SELECT track_json FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position",
        &[&id.to_string()],
    )
}

fn load_track_column(
    connection: &Connection,
    query: &str,
    params: &[&dyn rusqlite::ToSql],
) -> Result<Vec<TrackRef>> {
    let mut statement = connection.prepare(query)?;
    let rows = statement.query_map(params, |row| row.get::<_, String>(0))?;
    let mut tracks = Vec::new();
    for row in rows {
        tracks.push(decode_track(&row?)?);
    }
    Ok(tracks)
}

fn encode_track(track: &TrackRef) -> Result<String> {
    serde_json::to_string(track).context("Не удалось сохранить трек")
}

fn decode_track(value: &str) -> Result<TrackRef> {
    serde_json::from_str(value).context("В БД найден повреждённый трек")
}

const fn repeat_name(value: RepeatMode) -> &'static str {
    match value {
        RepeatMode::Off => "off",
        RepeatMode::All => "all",
        RepeatMode::One => "one",
    }
}

fn parse_repeat(value: &str) -> RepeatMode {
    match value {
        "all" => RepeatMode::All,
        "one" => RepeatMode::One,
        _ => RepeatMode::Off,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PlaybackCapability, ProviderKind};
    use url::Url;

    fn track(id: &str) -> TrackRef {
        TrackRef {
            provider: ProviderKind::SoundCloud,
            id: id.to_string(),
            title: format!("Трек {id}"),
            artists: vec!["Исполнитель".to_string()],
            duration_ms: Some(100_000),
            artwork_url: None,
            web_url: Url::parse("https://soundcloud.com/a/b").unwrap(),
            capability: PlaybackCapability::Full,
            genres: vec!["test".to_string()],
            explicit: false,
            drm: false,
        }
    }

    fn storage() -> (tempfile::TempDir, Storage) {
        let temp = tempfile::tempdir().unwrap();
        let storage = Storage::new(temp.path().join("library.sqlite3"));
        storage.initialize().unwrap();
        (temp, storage)
    }

    #[test]
    fn playlist_and_library_roundtrip_work() {
        let (_temp, storage) = storage();
        let mut playlist = Playlist::new("Плейлист", 10);
        playlist.push_unique(track("1"));
        storage.save_playlist(&playlist).unwrap();
        storage.like_track(&track("1"), 11).unwrap();
        let playlists = storage.list_playlists().unwrap();
        assert_eq!(playlists.len(), 1);
        assert_eq!(playlists[0].tracks, vec![track("1")]);
        assert_eq!(storage.library_tracks().unwrap(), vec![track("1")]);
    }

    #[test]
    fn history_and_queue_roundtrip_work() {
        let (_temp, storage) = storage();
        storage
            .record_history(&HistoryEntry {
                track: track("1"),
                played_at_ms: 20,
                completed: true,
                skipped: false,
            })
            .unwrap();
        let queue = QueueSnapshot {
            tracks: vec![track("1"), track("2")],
            current_index: Some(1),
            shuffle: true,
            repeat: RepeatMode::All,
        };
        storage.save_queue(&queue).unwrap();
        assert_eq!(storage.recent_history(10).unwrap().len(), 1);
        assert_eq!(storage.load_queue().unwrap(), queue);
    }

    #[test]
    fn wave_reads_real_like_timestamps_instead_of_inventing_them() {
        let (_temp, storage) = storage();
        let first = track("first");
        let second = track("second");
        storage.like_track(&first, 10).unwrap();
        storage.like_track(&second, 20).unwrap();
        assert_eq!(
            storage.liked_tracks_with_time().unwrap(),
            [(second, 20), (first, 10)]
        );
    }

    #[test]
    fn history_between_filters_both_date_boundaries() {
        let (_temp, storage) = storage();
        for played_at_ms in [999, 1_000, 1_999, 2_000] {
            storage
                .record_history(&HistoryEntry {
                    track: track(&played_at_ms.to_string()),
                    played_at_ms,
                    completed: false,
                    skipped: false,
                })
                .unwrap();
        }

        let entries = storage.history_between(1_000, 2_000, 10).unwrap();
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.played_at_ms)
                .collect::<Vec<_>>(),
            vec![1_999, 1_000]
        );
    }
}
