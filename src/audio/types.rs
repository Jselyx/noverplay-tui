#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AudioEvent {
    Buffering,
    Playing,
    Paused,
    Stopped,
    Ended,
    Failed(String),
    OutputFailed(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct AudioStatus {
    pub position_ms: u64,
    pub buffered_ms: u64,
    pub paused: bool,
    pub volume_percent: u8,
    pub output_name: String,
}
