//! 专辑详情。
//!
//! 接口：`POST /weapi/v1/album/{id}`（body 为空）。实测返回：
//! - 顶层 `songs`：曲目数组（`al/ar/dt` 结构，和歌单详情同一套解析）
//! - `album`：专辑元信息（`name` / `artists[]` / `publishTime`(ms) / `company` / `description` / `picUrl`）
//!
//! `/weapi/v3/album/{id}` 是 404，别用。

use crate::client::Client;
use crate::error::ApiError;
use crate::models::Song;
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct AlbumDetail {
    pub id: u64,
    pub name: String,
    pub artists: Vec<String>,
    /// 与 artists 一一对应，用来跳歌手页
    pub artist_ids: Vec<u64>,
    pub cover_url: Option<String>,
    /// 发行时间（毫秒时间戳）
    pub publish_ms: Option<u64>,
    pub company: String,
    pub description: String,
    pub songs: Vec<Song>,
}

pub async fn album(client: &Client, id: u64) -> Result<AlbumDetail, ApiError> {
    let path = format!("/weapi/v1/album/{id}");
    let json = client.weapi(&path, &json!({})).await?;
    parse(&json, id)
}

fn parse(json: &Value, fallback_id: u64) -> Result<AlbumDetail, ApiError> {
    let album = json.get("album").unwrap_or(json);

    let name = album
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let mut artists: Vec<String> = Vec::new();
    let mut artist_ids: Vec<u64> = Vec::new();
    if let Some(list) = album.get("artists").and_then(Value::as_array) {
        for item in list {
            let Some(name) = item.get("name").and_then(Value::as_str) else {
                continue;
            };
            artists.push(name.to_string());
            artist_ids.push(item.get("id").and_then(Value::as_u64).unwrap_or(0));
        }
    }

    // 老结构里可能只有单个 artist 对象
    let artists = if artists.is_empty() {
        album
            .get("artist")
            .and_then(|artist| artist.get("name"))
            .and_then(Value::as_str)
            .map(|name| vec![name.to_string()])
            .unwrap_or_default()
    } else {
        artists
    };

    let cover_url = album
        .get("picUrl")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let publish_ms = album.get("publishTime").and_then(Value::as_u64);
    let company = album
        .get("company")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let description = album
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let songs: Vec<Song> = json
        .get("songs")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(Song::from_json).collect())
        .unwrap_or_default();

    let id = album.get("id").and_then(Value::as_u64).unwrap_or(fallback_id);

    Ok(AlbumDetail {
        id,
        name,
        artists,
        artist_ids,
        cover_url,
        publish_ms,
        company,
        description,
        songs,
    })
}
