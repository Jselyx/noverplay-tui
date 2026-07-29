use anyhow::{Context, Result, bail};
use yandex_music::{
    YandexMusicClient, api::search::get_search::SearchOptions, model::search::SearchType,
};

pub(super) async fn resolve_user_id(client: &YandexMusicClient, login: &str) -> Result<u64> {
    let expected = login.trim().to_lowercase();
    let result = client
        .search(&SearchOptions::new(login).item_type(SearchType::All))
        .await
        .context("Yandex Music не нашел владельца плейлиста")?;
    if let Some(users) = result.users
        && let Some(user) = users.results.into_iter().find(|user| {
            user.login.eq_ignore_ascii_case(&expected)
                || user
                    .display_name
                    .as_deref()
                    .is_some_and(|name| name.eq_ignore_ascii_case(&expected))
        })
    {
        return Ok(user.uid);
    }
    if let Some(playlists) = result.playlists
        && let Some(playlist) = playlists
            .results
            .into_iter()
            .find(|playlist| playlist.owner.login.eq_ignore_ascii_case(&expected))
    {
        return Ok(playlist.owner.uid);
    }
    bail!("не удалось определить владельца плейлиста Yandex Music")
}
