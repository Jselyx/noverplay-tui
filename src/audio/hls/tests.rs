use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::TcpListener,
    thread,
};

use url::Url;

use super::manifest::{HlsByteRange, HlsPlaylist, parse_playlist};
use super::source::HlsSource;

#[test]
fn media_playlist_resolves_assets_and_ranges() {
    let base = Url::parse("https://cf-hls-media.sndcdn.com/music/playlist.m3u8").unwrap();
    let body = concat!(
        "#EXTM3U\n",
        "#EXT-X-MAP:URI=\"init.mp4\",BYTERANGE=\"120@0\"\n",
        "#EXTINF:5.5,\n",
        "media.mp4\n",
        "#EXT-X-BYTERANGE:300@120\n",
        "#EXTINF:4,\n",
        "media.mp4\n",
        "#EXT-X-BYTERANGE:250\n",
        "#EXTINF:3.25,\n",
        "media.mp4\n",
    );
    let HlsPlaylist::Media(playlist) = parse_playlist(&base, body).unwrap() else {
        panic!("ожидался media playlist")
    };
    assert_eq!(playlist.segments.len(), 3);
    assert_eq!(playlist.segments[0].duration_ms, 5_500);
    assert_eq!(
        playlist.segments[0].init.as_ref().unwrap().url.as_str(),
        "https://cf-hls-media.sndcdn.com/music/init.mp4"
    );
    assert_eq!(
        playlist.segments[2].asset.byte_range,
        Some(HlsByteRange {
            start: 420,
            length: 250
        })
    );
}

#[test]
fn master_playlist_keeps_variants() {
    let base = Url::parse("https://cf-hls-media.sndcdn.com/master.m3u8").unwrap();
    let body = concat!(
        "#EXTM3U\n",
        "#EXT-X-STREAM-INF:BANDWIDTH=96000,CODECS=\"mp4a.40.2\"\n",
        "low/playlist.m3u8\n",
        "#EXT-X-STREAM-INF:BANDWIDTH=160000,CODECS=\"mp4a.40.2\"\n",
        "high/playlist.m3u8\n",
    );
    let HlsPlaylist::Master(variants) = parse_playlist(&base, body).unwrap() else {
        panic!("ожидался master playlist")
    };
    assert_eq!(variants[1].bandwidth, 160_000);
    assert_eq!(
        variants[1].url.as_str(),
        "https://cf-hls-media.sndcdn.com/high/playlist.m3u8"
    );
}

#[test]
fn encrypted_playlist_is_rejected() {
    let base = Url::parse("https://cf-hls-media.sndcdn.com/playlist.m3u8").unwrap();
    let body = concat!(
        "#EXTM3U\n",
        "#EXT-X-KEY:METHOD=SAMPLE-AES,URI=\"skd://license\"\n",
        "#EXTINF:6,\n",
        "data000.m4s\n",
    );
    let error = parse_playlist(&base, body).unwrap_err();
    assert!(error.to_string().contains("зашифрованный HLS"));
}

#[test]
fn source_reads_segments_without_collecting_the_track() {
    let manifest = concat!(
        "#EXTM3U\n",
        "#EXTINF:5,\n",
        "one.aac\n",
        "#EXTINF:5,\n",
        "two.aac\n",
    );
    let (url, server) = spawn_server(vec![
        ("/playlist.m3u8", manifest.as_bytes().to_vec()),
        ("/one.aac", vec![0xff, 0xf1, 1, 2]),
        ("/two.aac", vec![0xff, 0xf1, 3, 4]),
    ]);
    let mut source = HlsSource::open(&url, &BTreeMap::new(), 0).unwrap();
    let mut bytes = Vec::new();
    source.read_to_end(&mut bytes).unwrap();
    server.join().unwrap();
    assert_eq!(source.extension(), "aac");
    assert_eq!(bytes, vec![0xff, 0xf1, 1, 2, 0xff, 0xf1, 3, 4]);
}

#[test]
fn source_starts_from_segment_near_seek_position() {
    let manifest = concat!(
        "#EXTM3U\n",
        "#EXTINF:5,\n",
        "one.aac\n",
        "#EXTINF:5,\n",
        "two.aac\n",
    );
    let (url, server) = spawn_server(vec![
        ("/playlist.m3u8", manifest.as_bytes().to_vec()),
        ("/two.aac", vec![0xff, 0xf1, 3, 4]),
    ]);
    let mut source = HlsSource::open(&url, &BTreeMap::new(), 7_000).unwrap();
    let mut bytes = Vec::new();
    source.read_to_end(&mut bytes).unwrap();
    server.join().unwrap();
    assert_eq!(source.start_ms(), 5_000);
    assert_eq!(bytes, vec![0xff, 0xf1, 3, 4]);
}

fn spawn_server(responses: Vec<(&str, Vec<u8>)>) -> (Url, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let responses = responses
        .into_iter()
        .map(|(path, body)| (path.to_string(), body))
        .collect::<Vec<_>>();
    let server = thread::spawn(move || {
        for (expected_path, body) in responses {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0u8; 4096];
            let read = socket.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            assert!(request.starts_with(&format!("GET {expected_path} ")));
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            socket.write_all(&body).unwrap();
        }
    });
    (
        Url::parse(&format!("http://{address}/playlist.m3u8")).unwrap(),
        server,
    )
}
