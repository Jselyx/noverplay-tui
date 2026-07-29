use crate::model::TrackRef;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppEffect {
    Search(String),
    Play(Box<TrackRef>),
    Pause,
    Resume,
    Seek(u64),
    SetVolume(u8),
    Stop,
}
