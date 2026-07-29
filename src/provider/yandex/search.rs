use anyhow::{Context, Result};
use yandex_music::{
    YandexMusicClient, api::search::get_search::SearchOptions, model::search::SearchType,
};

use crate::provider::SearchPage;

use super::normalizovat_track;

pub(super) async fn search_tracks(
    client: &YandexMusicClient,
    query: &str,
    cursor: Option<&str>,
) -> Result<SearchPage> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(SearchPage::default());
    }
    let page = cursor.and_then(|value| value.parse().ok()).unwrap_or(0);
    let response = client
        .search(
            &SearchOptions::new(query)
                .page(page)
                .item_type(SearchType::Tracks),
        )
        .await
        .context("Yandex Music не выполнил поиск")?;
    let Some(result) = response.tracks else {
        return Ok(SearchPage::default());
    };
    let loaded_before = page.saturating_mul(result.per_page);
    let next_cursor = (loaded_before + (result.results.len() as u32) < result.total)
        .then(|| (page + 1).to_string());

    // Реклама и пустые карточки идут к начальнику отряда, у нас музыкальная зона а не базар 🫩
    let tracks = result
        .results
        .into_iter()
        .filter_map(normalizovat_track)
        .collect();
    Ok(SearchPage {
        tracks,
        next_cursor,
    })
}
