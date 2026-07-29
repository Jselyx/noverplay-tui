use std::sync::Arc;

use tokio::{sync::mpsc::UnboundedSender, task::JoinHandle};

use crate::{model::TrackRef, provider::MusicProvider};

use super::message::RuntimeMessage;

pub(super) fn spawn_playback(
    provider: Arc<dyn MusicProvider>,
    sender: UnboundedSender<RuntimeMessage>,
    generation: u64,
    track: TrackRef,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let message = match provider.playback_source(&track).await {
            Ok(source) => RuntimeMessage::PlaybackReady { generation, source },
            Err(error) => RuntimeMessage::PlaybackFailed {
                generation,
                error: error.to_string(),
            },
        };
        let _ = sender.send(message);
    })
}
