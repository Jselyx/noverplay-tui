use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TrySendError},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

use crate::{
    config::AppPaths,
    model::{SearchProvider, TrackRef},
};

const MAX_MESSAGE_BYTES: usize = 64 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_millis(500);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);
const CONTROL_QUEUE_CAPACITY: usize = 32;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ControlRequest {
    pub token: String,
    pub command: ControlCommand,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum ControlCommand {
    Play {
        query: String,
        provider: SearchProvider,
    },
    Search {
        query: String,
        provider: SearchProvider,
    },
    Wave,
    Pause,
    Resume,
    Toggle,
    Next,
    Previous,
    Stop,
    QueueList,
    QueueAdd {
        query: String,
        provider: SearchProvider,
    },
    QueueRemove {
        index: usize,
    },
    QueueClear,
    Status,
    Shutdown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ControlResponse {
    pub ok: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<ResponseData>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ResponseData {
    Status(Box<StatusSnapshot>),
    Tracks(Vec<TrackRef>),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StatusSnapshot {
    pub playback: String,
    pub track: Option<TrackRef>,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub volume_percent: u8,
    pub queue_index: Option<usize>,
    pub queue_length: usize,
}

impl ControlResponse {
    pub fn accepted(message: impl Into<String>) -> Self {
        Self {
            ok: true,
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(message: impl Into<String>, data: ResponseData) -> Self {
        Self {
            ok: true,
            message: message.into(),
            data: Some(data),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            message: message.into(),
            data: None,
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "np",
    bin_name = "np",
    version,
    about = "Управление Noverplay, даже если TUI сейчас закрыт"
)]
pub struct NpCli {
    #[command(subcommand)]
    pub command: NpCommand,
}

#[derive(Debug, Subcommand)]
pub enum NpCommand {
    /// Найти и запустить первый доступный трек.
    Play(QueryArgs),
    /// Найти треки и вывести результаты.
    Search(QueryArgs),
    /// Собрать Мою волну, заменить ею очередь и начать воспроизведение.
    Wave,
    /// Поставить воспроизведение на паузу.
    Pause,
    /// Продолжить воспроизведение.
    Resume,
    /// Переключить паузу/воспроизведение.
    Toggle,
    /// Перейти к следующему треку.
    Next,
    /// Вернуться к предыдущему треку.
    Previous,
    /// Остановить воспроизведение.
    Stop,
    /// Просмотреть или изменить очередь.
    Queue(QueueArgs),
    /// Показать состояние запущенного плеера.
    Status(StatusArgs),
    /// Показать историю прослушивания.
    History(HistoryArgs),
}

#[derive(Clone, Debug, Args)]
pub struct QueryArgs {
    #[arg(required = true, num_args = 1..)]
    pub query: Vec<String>,
    #[arg(long, value_enum, default_value_t, help = "Площадка поиска")]
    pub provider: ProviderArg,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum ProviderArg {
    #[default]
    All,
    #[value(alias = "sc")]
    Soundcloud,
    #[value(alias = "ya", alias = "ym")]
    Yandex,
    #[value(alias = "dz")]
    Deezer,
}

impl From<ProviderArg> for SearchProvider {
    fn from(value: ProviderArg) -> Self {
        match value {
            ProviderArg::All => Self::All,
            ProviderArg::Soundcloud => Self::SoundCloud,
            ProviderArg::Yandex => Self::YandexMusic,
            ProviderArg::Deezer => Self::Deezer,
        }
    }
}

#[derive(Debug, Args)]
pub struct QueueArgs {
    #[command(subcommand)]
    pub command: QueueCommand,
}

#[derive(Debug, Subcommand)]
pub enum QueueCommand {
    /// Показать текущую очередь.
    List,
    /// Найти первый доступный трек и добавить в конец.
    Add(QueryArgs),
    /// Удалить трек по позиции (начиная с 1).
    Remove {
        #[arg(value_parser = parse_positive_usize)]
        index: usize,
    },
    /// Очистить очередь и остановить воспроизведение.
    Clear,
}

#[derive(Debug, Args)]
pub struct StatusArgs {
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct HistoryArgs {
    #[command(subcommand)]
    pub command: HistoryCommand,
}

#[derive(Debug, Subcommand)]
pub enum HistoryCommand {
    /// История за текущий локальный календарный день.
    Today(HistoryFormat),
    /// Последние записи истории.
    Recent(RecentHistoryArgs),
}

#[derive(Debug, Args)]
pub struct HistoryFormat {
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct RecentHistoryArgs {
    #[arg(default_value_t = 20, value_parser = parse_history_limit)]
    pub limit: usize,
    #[arg(long)]
    pub json: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlOwner {
    #[default]
    Interactive,
    Background,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct Endpoint {
    address: SocketAddr,
    token: String,
    pid: u32,
    #[serde(default)]
    owner: ControlOwner,
}

pub struct ControlServer {
    receiver: Receiver<(TcpStream, ControlRequest)>,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    endpoint_path: PathBuf,
    endpoint: Endpoint,
}

impl ControlServer {
    pub fn bind(paths: &AppPaths) -> Result<Self> {
        Self::bind_as(paths, ControlOwner::Interactive)
    }

    pub fn bind_background(paths: &AppPaths) -> Result<Self> {
        Self::bind_as(paths, ControlOwner::Background)
    }

    fn bind_as(paths: &AppPaths, owner: ControlOwner) -> Result<Self> {
        remove_stale_endpoint(&paths.control_endpoint_file)?;
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .context("Не удалось открыть локальный control socket")?;
        listener.set_nonblocking(true)?;
        let token = random_token();
        let endpoint = Endpoint {
            address: listener.local_addr()?,
            token: token.clone(),
            pid: std::process::id(),
            owner,
        };
        write_endpoint(&paths.control_endpoint_file, &endpoint)?;
        let (sender, receiver) = mpsc::sync_channel(CONTROL_QUEUE_CAPACITY);
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_shutdown = Arc::clone(&shutdown);
        let worker_token = token;
        let thread = match thread::Builder::new()
            .name("noverplay-control".to_string())
            .spawn(move || {
                while !worker_shutdown.load(Ordering::Relaxed) {
                    let (mut stream, _) = match listener.accept() {
                        Ok(value) => value,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(10));
                            continue;
                        }
                        Err(_) => break,
                    };
                    let request = read_request(&mut stream).and_then(|request| {
                        if request.token == worker_token {
                            Ok(request)
                        } else {
                            bail!("control token отклонён")
                        }
                    });
                    match request {
                        Ok(request) => match sender.try_send((stream, request)) {
                            Ok(()) => {}
                            Err(TrySendError::Full((mut stream, _))) => {
                                let _ = send_response(
                                    &mut stream,
                                    &ControlResponse::error("Очередь control-команд переполнена"),
                                );
                            }
                            Err(TrySendError::Disconnected(_)) => break,
                        },
                        Err(error) => {
                            let _ = send_response(
                                &mut stream,
                                &ControlResponse::error(error.to_string()),
                            );
                        }
                    }
                }
            }) {
            Ok(thread) => thread,
            Err(error) => {
                remove_endpoint_if_matches(&paths.control_endpoint_file, &endpoint);
                return Err(error).context("Не удалось запустить control thread");
            }
        };
        Ok(Self {
            receiver,
            shutdown,
            thread: Some(thread),
            endpoint_path: paths.control_endpoint_file.clone(),
            endpoint,
        })
    }

    pub fn poll(&self) -> Vec<(TcpStream, ControlRequest)> {
        self.receiver.try_iter().collect()
    }
}

pub fn active_control_owner(paths: &AppPaths) -> Option<ControlOwner> {
    let endpoint = read_endpoint(&paths.control_endpoint_file).ok()?;
    if TcpStream::connect_timeout(&endpoint.address, Duration::from_millis(150)).is_ok() {
        Some(endpoint.owner)
    } else {
        remove_endpoint_if_matches(&paths.control_endpoint_file, &endpoint);
        None
    }
}

impl Drop for ControlServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        remove_endpoint_if_matches(&self.endpoint_path, &self.endpoint);
    }
}

pub fn send_command(paths: &AppPaths, command: ControlCommand) -> Result<ControlResponse> {
    let endpoint = read_endpoint(&paths.control_endpoint_file)?;
    let mut stream = TcpStream::connect_timeout(&endpoint.address, REQUEST_TIMEOUT)
        .map_err(|error| stale_error(&paths.control_endpoint_file, &endpoint, error))?;
    stream.set_read_timeout(Some(RESPONSE_TIMEOUT))?;
    stream.set_write_timeout(Some(REQUEST_TIMEOUT))?;
    let request = ControlRequest {
        token: endpoint.token.clone(),
        command,
    };
    let result = (|| -> Result<ControlResponse> {
        let bytes = serde_json::to_vec(&request)?;
        if bytes.len() > MAX_MESSAGE_BYTES {
            bail!("control request слишком большой");
        }
        stream.write_all(&bytes)?;
        stream.write_all(b"\n")?;
        stream.flush()?;
        let mut reader = BufReader::new(stream);
        let mut response = Vec::new();
        reader
            .by_ref()
            .take(MAX_MESSAGE_BYTES as u64 + 1)
            .read_until(b'\n', &mut response)?;
        if response.len() > MAX_MESSAGE_BYTES {
            bail!("control response слишком большой");
        }
        serde_json::from_slice(&response).context("TUI вернул повреждённый control response")
    })();
    if result.is_err() {
        remove_endpoint_if_matches(&paths.control_endpoint_file, &endpoint);
    }
    result
}

pub fn send_response(stream: &mut TcpStream, response: &ControlResponse) -> Result<()> {
    stream.set_write_timeout(Some(REQUEST_TIMEOUT))?;
    serde_json::to_writer(&mut *stream, response)?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    Ok(())
}

pub fn split_provider_tag(
    words: &[String],
    explicit: ProviderArg,
) -> Result<(String, SearchProvider)> {
    let explicit_provider = SearchProvider::from(explicit);
    let mut tagged_provider = None;
    let mut query = Vec::new();
    for word in words {
        let tagged = match word.to_ascii_lowercase().as_str() {
            "@sc" | "#sc" | "@soundcloud" | "#soundcloud" => Some(SearchProvider::SoundCloud),
            "@ya" | "#ya" | "@ym" | "#ym" | "@yandex" | "#yandex" => {
                Some(SearchProvider::YandexMusic)
            }
            "@dz" | "#dz" | "@deezer" | "#deezer" => Some(SearchProvider::Deezer),
            _ => None,
        };
        if let Some(tagged) = tagged {
            if tagged_provider.is_some_and(|current| current != tagged) {
                bail!("в запросе указано несколько разных provider tags");
            }
            if explicit != ProviderArg::All && explicit_provider != tagged {
                bail!("provider tag конфликтует с --provider");
            }
            tagged_provider = Some(tagged);
        } else {
            query.push(word.as_str());
        }
    }
    let query = query.join(" ").trim().to_string();
    if query.is_empty() {
        bail!("поисковый запрос пуст");
    }
    Ok((query, tagged_provider.unwrap_or(explicit_provider)))
}

fn read_request(stream: &mut TcpStream) -> Result<ControlRequest> {
    stream.set_read_timeout(Some(REQUEST_TIMEOUT))?;
    let mut reader = BufReader::new(stream);
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take(MAX_MESSAGE_BYTES as u64 + 1)
        .read_until(b'\n', &mut bytes)?;
    if bytes.len() > MAX_MESSAGE_BYTES {
        bail!("control request слишком большой");
    }
    serde_json::from_slice(&bytes).context("повреждённый control request")
}

fn write_endpoint(path: &Path, endpoint: &Endpoint) -> Result<()> {
    let parent = path
        .parent()
        .context("control endpoint остался без родительского каталога")?;
    fs::create_dir_all(parent)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .with_context(|| "Noverplay уже запущен или endpoint занят")?;
    let result = (|| -> Result<()> {
        file.write_all(&serde_json::to_vec(endpoint)?)?;
        file.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(path);
    }
    result
}

fn read_endpoint(path: &Path) -> Result<Endpoint> {
    let bytes =
        fs::read(path).with_context(|| "Noverplay не запущен (control endpoint не найден)")?;
    let endpoint: Endpoint =
        serde_json::from_slice(&bytes).context("control endpoint повреждён")?;
    if !endpoint.address.ip().is_loopback() {
        bail!("control endpoint не локальный");
    }
    Ok(endpoint)
}

fn remove_stale_endpoint(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let original = fs::read(path)?;
    let stale = serde_json::from_slice::<Endpoint>(&original)
        .ok()
        .filter(|endpoint| endpoint.address.ip().is_loopback())
        .is_none_or(|endpoint| {
            TcpStream::connect_timeout(&endpoint.address, Duration::from_millis(150)).is_err()
        });
    if stale && fs::read(path).ok().as_deref() == Some(original.as_slice()) {
        fs::remove_file(path)
            .with_context(|| format!("Не удалось удалить stale endpoint {}", path.display()))?;
    }
    if path.exists() {
        bail!("Noverplay уже запущен");
    }
    Ok(())
}

fn stale_error(path: &Path, endpoint: &Endpoint, error: std::io::Error) -> anyhow::Error {
    remove_endpoint_if_matches(path, endpoint);
    anyhow!("Noverplay не отвечает; stale control endpoint удалён: {error}")
}

fn remove_endpoint_if_matches(path: &Path, endpoint: &Endpoint) {
    if read_endpoint(path).ok().as_ref() == Some(endpoint) {
        let _ = fs::remove_file(path);
    }
}

fn random_token() -> String {
    use rand_core::{OsRng, RngCore};
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn parse_positive_usize(value: &str) -> std::result::Result<usize, String> {
    let value = value
        .parse::<usize>()
        .map_err(|_| "ожидалось положительное целое число".to_string())?;
    (value > 0)
        .then_some(value)
        .ok_or_else(|| "значение должно начинаться с 1".to_string())
}

fn parse_history_limit(value: &str) -> std::result::Result<usize, String> {
    let value = parse_positive_usize(value)?;
    (value <= 10_000)
        .then_some(value)
        .ok_or_else(|| "лимит не может быть больше 10000".to_string())
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    #[test]
    fn background_server_forwards_one_authenticated_request() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_roots(
            temp.path().join("config"),
            temp.path().join("data"),
            temp.path().join("cache"),
        );
        paths.ensure().unwrap();
        let server = ControlServer::bind(&paths).unwrap();
        let client_paths = paths.clone();
        let client = thread::spawn(move || send_command(&client_paths, ControlCommand::Status));

        let deadline = Instant::now() + Duration::from_secs(2);
        let (mut stream, request) = loop {
            if let Some(request) = server.poll().into_iter().next() {
                break request;
            }
            assert!(
                Instant::now() < deadline,
                "control request was not forwarded"
            );
            thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(request.command, ControlCommand::Status);
        send_response(&mut stream, &ControlResponse::accepted("ok")).unwrap();
        assert_eq!(
            client.join().unwrap().unwrap(),
            ControlResponse::accepted("ok")
        );

        drop(server);
        assert!(!paths.control_endpoint_file.exists());
    }

    #[test]
    fn old_owner_cannot_remove_a_replaced_endpoint() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("control.json");
        let old = Endpoint {
            address: "127.0.0.1:10001".parse().unwrap(),
            token: "old".to_string(),
            pid: 1,
            owner: ControlOwner::Interactive,
        };
        let replacement = Endpoint {
            address: "127.0.0.1:10002".parse().unwrap(),
            token: "new".to_string(),
            pid: 2,
            owner: ControlOwner::Background,
        };
        fs::write(&path, serde_json::to_vec(&replacement).unwrap()).unwrap();
        remove_endpoint_if_matches(&path, &old);
        assert_eq!(read_endpoint(&path).unwrap(), replacement);
    }

    #[test]
    fn endpoint_from_old_release_defaults_to_interactive_owner() {
        let endpoint: Endpoint =
            serde_json::from_str(r#"{"address":"127.0.0.1:10001","token":"old","pid":1}"#).unwrap();

        assert_eq!(endpoint.owner, ControlOwner::Interactive);
    }
}
