use anyhow::{Result, ensure};
use url::Url;

pub fn proverit_soundcloud_url(url: &Url) -> Result<()> {
    let host = url
        .host_str()
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    ensure!(
        host == "soundcloud.com" || host.ends_with(".soundcloud.com"),
        "ссылка ведет не на SoundCloud"
    );
    ensure!(
        url.scheme() == "https",
        "SoundCloud ссылка должна быть HTTPS"
    );
    Ok(())
}
