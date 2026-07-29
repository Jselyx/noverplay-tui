use crate::model::TrackRef;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppEffect {
    Search { query: String, immediate: bool },
    Play(Box<TrackRef>),
    Pause,
    Resume,
    Seek(u64),
    SetVolume(u8),
    Stop,
}
