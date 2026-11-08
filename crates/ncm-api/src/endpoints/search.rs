use crate::client::Client;
use crate::error::ApiError;
use crate::models::Song;
use serde_json::{json, Value};

/// 综合搜索（type=1 单曲）。
///
/// 走 `cloudsearch`：它返回的是 `al` / `ar` / `dt` 结构，**带封面 picUrl**；
/// `/search/get` 只给 `album.picId`，拿不到图。老接口留作兜底。
pub async fn search(client: &Client, keyword: &str, limit: u32) -> Result<Vec<Song>, ApiError> {
    let cloud = client
        .weapi(
            "/weapi/cloudsearch/pc",
            &json!({ "s": keyword, "type": 1, "limit": limit, "offset": 0 }),
        )
        .await;

    let songs = match cloud {
        Ok(json) => json
            .pointer("/result/songs")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    };

    if !songs.is_empty() {
        return Ok(songs.iter().filter_map(Song::from_json).collect());
    }

    // 兜底：老接口（没有封面，但能搜到）
    let json = client
        .weapi(
            "/weapi/search/get",
            &json!({ "s": keyword, "type": 1, "limit": limit, "offset": 0, "total": true }),
        )
        .await?;

    let fallback = json
        .pointer("/result/songs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    Ok(fallback.iter().filter_map(Song::from_json).collect())
}
