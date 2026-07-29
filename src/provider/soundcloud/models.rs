use serde::{Deserialize, Deserializer};

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ScUser {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub permalink_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ScFormat {
    pub protocol: String,
    pub mime_type: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ScTranscoding {
    pub url: String,
    pub preset: String,
    #[serde(default)]
    pub snipped: bool,
    pub format: ScFormat,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ScMedia {
    #[serde(default)]
    pub transcodings: Vec<ScTranscoding>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ScTrack {
    #[serde(deserialize_with = "string_id")]
    pub id: String,
    #[serde(default)]
    pub urn: Option<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub duration: Option<u64>,
    #[serde(default)]
    pub artwork_url: Option<String>,
    #[serde(default)]
    pub permalink_url: Option<String>,
    #[serde(default)]
    pub genre: Option<String>,
    #[serde(default)]
    pub tag_list: String,
    #[serde(default)]
    pub access: Option<String>,
    #[serde(default)]
    pub policy: Option<String>,
    #[serde(default)]
    pub streamable: Option<bool>,
    #[serde(default)]
    pub media: ScMedia,
    #[serde(default)]
    pub user: ScUser,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ScPlaylist {
    #[serde(deserialize_with = "string_id")]
    pub id: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub permalink_url: Option<String>,
    #[serde(default)]
    pub tracks: Vec<ScTrack>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
pub struct ScCollection<T> {
    #[serde(default)]
    pub collection: Vec<T>,
    #[serde(default)]
    pub next_href: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ScResolvedStream {
    pub url: String,
}

fn string_id<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::String(value) => Ok(value),
        serde_json::Value::Number(value) => Ok(value.to_string()),
        _ => Err(serde::de::Error::custom(
            "ожидался строковый или числовой id",
        )),
    }
}
