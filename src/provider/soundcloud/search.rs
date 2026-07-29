use anyhow::Result;

use crate::provider::SearchPage;

use super::{client::SoundCloudClient, models::ScCollection, normalizovat_track};

const SEARCH_LIMIT: usize = 50;

pub(super) async fn search_tracks(
    client: &SoundCloudClient,
    query: &str,
    cursor: Option<&str>,
) -> Result<SearchPage> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(SearchPage::default());
    }
    let offset = cursor
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let response: ScCollection<_> = client
        .get_json(
            client.v2_url(&["search", "tracks"])?,
            &[
                ("q", query.to_string()),
                ("limit", SEARCH_LIMIT.to_string()),
                ("offset", offset.to_string()),
                ("linked_partitioning", "true".to_string()),
                ("access", "playable,preview".to_string()),
            ],
        )
        .await?;
    // Чужой next_href не тащим, а то api однажды подсунет чемодан без ручки и вся зона поедет за ним
    let has_more = response.next_href.is_some();
    let tracks = response
        .collection
        .into_iter()
        .filter_map(normalizovat_track)
        .collect();
    Ok(SearchPage {
        tracks,
        next_cursor: has_more.then(|| (offset + SEARCH_LIMIT).to_string()),
    })
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    use url::Url;

    use super::*;

    #[tokio::test]
    async fn search_works_against_local_mock_without_real_key() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0u8; 4096];
            let read = socket.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            assert!(request.contains("GET /search/tracks?"));
            assert!(request.contains("client_id=fake-key"));
            let body = r#"{"collection":[{"id":"42","title":"Mock","access":"playable","permalink_url":"https://soundcloud.com/mock/track","user":{"username":"Mocker"}}],"next_href":"https://api-v2.soundcloud.com/search/tracks?offset=50"}"#;
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let base = Url::parse(&format!("http://{address}/")).unwrap();
        let client = SoundCloudClient::with_base("fake-key".to_string(), base).unwrap();
        let page = search_tracks(&client, "mock", None).await.unwrap();
        server.join().unwrap();
        assert_eq!(page.tracks.len(), 1);
        assert_eq!(page.tracks[0].title, "Mock");
        assert_eq!(page.next_cursor.as_deref(), Some("50"));
    }
}
