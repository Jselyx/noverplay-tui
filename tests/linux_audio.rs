#![cfg(target_os = "linux")]

use noverplay_tui::audio::{AudioEngine, AudioEvent};

#[test]
fn linux_build_includes_pulseaudio_backend() {
    assert!(
        cpal::ALL_HOSTS
            .iter()
            .any(|host| host.name() == "PulseAudio")
    );
}

#[test]
#[ignore = "requires a running desktop audio server"]
fn desktop_audio_output_opens_and_runs() {
    let audio = AudioEngine::new(None, 0).expect("open desktop audio output");
    assert!(!audio.status().output_name.is_empty());
    std::thread::sleep(std::time::Duration::from_millis(100));
    while let Some(event) = audio.try_event() {
        assert!(!matches!(event, AudioEvent::OutputFailed(_)), "{event:?}");
    }
}
