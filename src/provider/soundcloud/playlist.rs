use anyhow::{Context, Result, ensure};
use url::Url;

use crate::provider::ImportedPlaylist;

use super::{
    client::SoundCloudClient, models::ScPlaylist, normalizovat_track, proverit_soundcloud_url,
    track_details::zagruzit_dannye_trekov,
};

pub(super) async fn import_playlist(
    client: &SoundCloudClient,
    source_url: &Url,
) -> Result<ImportedPlaylist> {
    proverit_soundcloud_url(source_url)?;
    let playlist: ScPlaylist = client
        .get_json(
            client.v2_url(&["resolve"])?,
            &[("url", source_url.as_str().to_string())],
        )
        .await
        .context("SoundCloud не разобрал ссылку на плейлист")?;
    ensure!(
        matches!(playlist.kind.as_str(), "playlist" | "system-playlist"),
        "ссылка SoundCloud ведет не на плейлист"
    );

    let tracks = zagruzit_dannye_trekov(client, playlist.tracks)
        .await?
        .into_iter()
        .filter_map(normalizovat_track)
        .collect();
    let title = if playlist.title.trim().is_empty() {
        "Плейлист SoundCloud".to_string()
    } else {
        playlist.title
    };

    Ok(ImportedPlaylist {
        title,
        description: playlist.description.unwrap_or_default(),
        source_url: source_url.clone(),
        tracks,
    })
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::{Shutdown, TcpListener, TcpStream},
        thread,
    };

    use super::*;

    #[tokio::test]
    async fn import_hydrates_partial_tracks_and_keeps_order() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            for _ in 0..2 {
                let (mut socket, _) = listener.accept().unwrap();
                answer(&mut socket);
            }
        });
        let client = SoundCloudClient::with_base(
            "fake-key".to_string(),
            Url::parse(&format!("http://{address}/")).unwrap(),
        )
        .unwrap();
        let source = Url::parse("https://soundcloud.com/test/set").unwrap();
        let imported = import_playlist(&client, &source).await.unwrap();
        server.join().unwrap();

        assert_eq!(imported.title, "Набор");
        assert_eq!(imported.tracks.len(), 2);
        assert_eq!(imported.tracks[0].id, "42");
        assert_eq!(imported.tracks[0].artists, vec!["Автор"]);
        assert_eq!(imported.tracks[1].id, "9");
    }

    fn answer(socket: &mut TcpStream) {
        let mut request = [0u8; 4096];
        let read = socket.read(&mut request).unwrap();
        let request = String::from_utf8_lossy(&request[..read]);
        let body = if request.contains("GET /resolve?") {
            r#"{"id":7,"kind":"playlist","title":"Набор","tracks":[{"id":42},{"id":9,"title":"Готовый","permalink_url":"https://soundcloud.com/test/ready","user":{"username":"Другой"}}]}"#
        } else {
            assert!(request.contains("GET /tracks?"));
            assert!(request.contains("ids=42"));
            r#"[{"id":42,"title":"Раскрытый","permalink_url":"https://soundcloud.com/test/full","user":{"username":"Автор"}}]"#
        };
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        socket.write_all(response.as_bytes()).unwrap();
        socket.flush().unwrap();
        socket.shutdown(Shutdown::Write).unwrap();
    }
}
