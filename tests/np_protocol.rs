use clap::Parser;
use noverplay_tui::control::{
    ControlCommand, ControlRequest, NpCli, NpCommand, ProviderArg, split_provider_tag,
};
use noverplay_tui::model::SearchProvider;

#[test]
fn play_parser_accepts_provider_and_query_words() {
    let cli = NpCli::try_parse_from([
        "np",
        "play",
        "massive",
        "attack",
        "--provider",
        "soundcloud",
    ])
    .unwrap();
    assert!(matches!(
        cli.command,
        NpCommand::Play(args)
            if args.query == ["massive", "attack"] && args.provider == ProviderArg::Soundcloud
    ));
}

#[test]
fn play_parser_preserves_a_quoted_unicode_query() {
    let cli = NpCli::try_parse_from(["np", "play", "пошлый молли"]).unwrap();
    assert!(matches!(
        cli.command,
        NpCommand::Play(args)
            if args.query == ["пошлый молли"] && args.provider == ProviderArg::All
    ));
}

#[test]
fn protocol_roundtrip_preserves_tagged_command() {
    let request = ControlRequest {
        token: "secret".to_string(),
        command: ControlCommand::Play {
            query: "track".to_string(),
            provider: SearchProvider::Deezer,
        },
    };
    let json = serde_json::to_string(&request).unwrap();
    assert_eq!(
        serde_json::from_str::<ControlRequest>(&json).unwrap(),
        request
    );
}

#[test]
fn provider_suffix_defaults_to_all_and_overrides_it() {
    assert_eq!(
        split_provider_tag(&["massive".into(), "attack".into()], ProviderArg::All).unwrap(),
        ("massive attack".into(), SearchProvider::All)
    );
    assert_eq!(
        split_provider_tag(&["massive".into(), "@sc".into()], ProviderArg::All).unwrap(),
        ("massive".into(), SearchProvider::SoundCloud)
    );
}

#[test]
fn conflicting_provider_flag_and_suffix_is_rejected() {
    let error =
        split_provider_tag(&["track".into(), "@dz".into()], ProviderArg::Yandex).unwrap_err();
    assert!(error.to_string().contains("конфликтует"));
}

#[test]
fn conflicting_provider_tags_are_rejected_and_long_tags_work() {
    let error = split_provider_tag(
        &["track".into(), "@soundcloud".into(), "#deezer".into()],
        ProviderArg::All,
    )
    .unwrap_err();
    assert!(error.to_string().contains("несколько"));
    assert_eq!(
        split_provider_tag(&["track".into(), "#yandex".into()], ProviderArg::All).unwrap(),
        ("track".into(), SearchProvider::YandexMusic)
    );
}

#[test]
fn status_json_flag_and_all_queue_commands_parse() {
    assert!(NpCli::try_parse_from(["np", "status", "--json"]).is_ok());
    for args in [
        vec!["np", "queue", "list"],
        vec!["np", "queue", "add", "track"],
        vec!["np", "queue", "remove", "1"],
        vec!["np", "queue", "clear"],
    ] {
        assert!(NpCli::try_parse_from(args).is_ok());
    }
    assert!(NpCli::try_parse_from(["np", "queue", "remove", "0"]).is_err());
}
