//! 歌手详情与专辑列表。
//!
//! 探针实测：
//! - `POST /weapi/v1/artist/{id}` → `artist`（`name`/`picUrl`/`briefDesc`/`albumSize`/`musicSize`）+ `hotSongs`（50 首）
//! - `POST /weapi/artist/albums/{id}`，参数 `{ limit, offset, total }` → `hotAlbums` + `more`

use crate::client::Client;
use crate::error::ApiError;
use crate::models::Song;
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct ArtistDetail {
    pub id: u64,
    pub name: String,
    pub cover_url: Option<String>,
    pub brief_desc: String,
    pub album_count: u64,
    pub song_count: u64,
    pub hot_songs: Vec<Song>,
}

#[derive(Debug, Clone)]
pub struct ArtistAlbum {
    pub id: u64,
    pub name: String,
    pub cover_url: Option<String>,
    pub publish_ms: Option<u64>,
    pub size: u64,
    pub company: String,
}

pub async fn artist(client: &Client, id: u64) -> Result<ArtistDetail, ApiError> {
    let path = format!("/weapi/v1/artist/{id}");
    let json = client.weapi(&path, &json!({})).await?;
    let artist = json.get("artist");

    let name = artist
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let cover_url = artist
        .and_then(|value| value.get("picUrl"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let brief_desc = artist
        .and_then(|value| value.get("briefDesc"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let album_count = artist
        .and_then(|value| value.get("albumSize"))
        .and_then(Value::as_u64)
        .unwrap_or(0);

    let song_count = artist
        .and_then(|value| value.get("musicSize"))
        .and_then(Value::as_u64)
        .unwrap_or(0);

    let hot_songs: Vec<Song> = json
        .get("hotSongs")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(Song::from_json).collect())
        .unwrap_or_default();

    Ok(ArtistDetail {
        id,
        name,
        cover_url,
        brief_desc,
        album_count,
        song_count,
        hot_songs,
    })
}

/// 返回 (专辑列表, 是否有下一页)。
pub async fn artist_albums(
    client: &Client,
    id: u64,
    limit: u32,
    offset: u32,
) -> Result<(Vec<ArtistAlbum>, bool), ApiError> {
    let path = format!("/weapi/artist/albums/{id}");
    let json = client
        .weapi(
            &path,
            &json!({ "limit": limit, "offset": offset, "total": true }),
        )
        .await?;

    let albums: Vec<ArtistAlbum> = json
        .get("hotAlbums")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|item| {
                    let id = item.get("id").and_then(Value::as_u64)?;
                    Some(ArtistAlbum {
                        id,
                        name: item
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        cover_url: item
                            .get("picUrl")
                            .and_then(Value::as_str)
                            .filter(|value| !value.is_empty())
                            .map(str::to_string),
                        publish_ms: item.get("publishTime").and_then(Value::as_u64),
                        size: item.get("size").and_then(Value::as_u64).unwrap_or(0),
                        company: item
                            .get("company")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let more = json.get("more").and_then(Value::as_bool).unwrap_or(false);
    Ok((albums, more))
}
