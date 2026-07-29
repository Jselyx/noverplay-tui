use std::time::Duration;

use reqwest::{Client, StatusCode, redirect::Policy};
use url::Url;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SoundCloudAccess {
    Reachable { status: u16 },
    Unreachable { reason: String },
}

impl SoundCloudAccess {
    pub fn is_reachable(&self) -> bool {
        matches!(self, Self::Reachable { .. })
    }
}

pub async fn probe_soundcloud() -> SoundCloudAccess {
    let url = Url::parse("https://api-v2.soundcloud.com/").expect("статический URL правильный");
    probe_url(url, Duration::from_secs(6)).await
}

async fn probe_url(url: Url, timeout: Duration) -> SoundCloudAccess {
    let client = match Client::builder()
        .user_agent(format!("noverplay-tui/{}", crate::APP_VERSION))
        .redirect(Policy::limited(3))
        .timeout(timeout)
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return SoundCloudAccess::Unreachable {
                reason: format!("не удалось подготовить проверку: {error}"),
            };
        }
    };
    match client.get(url).send().await {
        Ok(response) if response.status() != StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS => {
            // даже 401 значит что API жив, ключи сейчас не экзаменуем и цирк с Zapret не открываем
            SoundCloudAccess::Reachable {
                status: response.status().as_u16(),
            }
        }
        Ok(response) => SoundCloudAccess::Unreachable {
            reason: format!("SoundCloud вернул HTTP {}", response.status().as_u16()),
        },
        Err(error) => SoundCloudAccess::Unreachable {
            reason: if error.is_timeout() {
                "SoundCloud не ответил вовремя".to_string()
            } else if error.is_connect() {
                "не удалось подключиться к SoundCloud".to_string()
            } else {
                format!("SoundCloud недоступен: {error}")
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    use super::*;

    #[tokio::test]
    async fn any_real_api_response_counts_as_access() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0_u8; 1024];
            let _ = socket.read(&mut request).unwrap();
            socket
                .write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
        });

        let access = probe_url(
            Url::parse(&format!("http://{address}/")).unwrap(),
            Duration::from_secs(1),
        )
        .await;

        server.join().unwrap();
        assert_eq!(access, SoundCloudAccess::Reachable { status: 401 });
    }

    #[tokio::test]
    async fn legal_block_opens_the_zapret_branch() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0_u8; 1024];
            let _ = socket.read(&mut request).unwrap();
            socket
                .write_all(
                    b"HTTP/1.1 451 Unavailable For Legal Reasons\r\nContent-Length: 0\r\n\r\n",
                )
                .unwrap();
        });

        let access = probe_url(
            Url::parse(&format!("http://{address}/")).unwrap(),
            Duration::from_secs(1),
        )
        .await;

        server.join().unwrap();
        assert!(!access.is_reachable());
        assert!(matches!(access, SoundCloudAccess::Unreachable { .. }));
    }

    #[tokio::test]
    async fn dead_socket_does_not_hang_the_first_run() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);

        let access = probe_url(
            Url::parse(&format!("http://{address}/")).unwrap(),
            Duration::from_millis(300),
        )
        .await;

        assert!(!access.is_reachable());
    }
}
