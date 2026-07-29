use reqwest::header::{CONTENT_RANGE, HeaderMap, HeaderValue};

use super::{convert::convert_audio, http_source::dlina_iz_range};

#[test]
fn mono_is_duplicated_for_stereo() {
    let output = convert_audio(&[0.25, 0.5], 1, 48_000, 2, 48_000);
    assert_eq!(output, vec![0.25, 0.25, 0.5, 0.5]);
}

#[test]
fn stereo_is_mixed_to_mono() {
    let output = convert_audio(&[1.0, -1.0, 0.5, 0.5], 2, 44_100, 1, 44_100);
    assert_eq!(output, vec![0.0, 0.5]);
}

#[test]
fn resampler_changes_frame_count() {
    let input = vec![0.0; 2 * 24_000];
    let output = convert_audio(&input, 2, 24_000, 2, 48_000);
    assert_eq!(output.len(), 2 * 48_000);
}

#[test]
fn content_range_yields_total_length() {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_RANGE, HeaderValue::from_static("bytes 0-255/12345"));
    assert_eq!(dlina_iz_range(&headers), Some(12_345));
}
