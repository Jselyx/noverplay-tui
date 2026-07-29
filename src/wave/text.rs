use crate::model::TrackRef;

pub(crate) fn artist_id(track: &TrackRef) -> String {
    normalize_search_text(&track.display_artist())
}

pub(crate) fn track_key(track: &TrackRef) -> String {
    track.provider_key()
}

pub(crate) fn normalize_search_text(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut needs_space = false;
    for symbol in value.chars().flat_map(char::to_lowercase) {
        if symbol.is_alphanumeric() {
            if needs_space && !normalized.is_empty() {
                normalized.push(' ');
            }
            normalized.push(symbol);
            needs_space = false;
        } else {
            needs_space = true;
        }
    }
    normalized
}
