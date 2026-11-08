use crate::client::Client;
use crate::error::ApiError;
use crate::models::Song;
use serde_json::{json, Value};

/// 每日推荐。
///
/// 主接口 `/weapi/v3/discover/recommend` 返回 `data.dailySongs`；
/// 老接口 `/weapi/v1/discovery/recommend/songs` 返回 `recommend`。
/// 两个都试，谁先给出结果用谁——日推是「今天听什么」的入口，不该因为
/// 某个接口调整就整页空白。
pub async fn daily_songs(client: &Client) -> Result<Vec<Song>, ApiError> {
    let primary = client
        .weapi("/weapi/v3/discover/recommend", &json!({}))
        .await;

    if let Ok(json) = primary {
        if let Some(songs) = extract(&json, "/data/dailySongs") {
            if !songs.is_empty() {
                return Ok(songs);
            }
        }
    }

    let fallback = client
        .weapi("/weapi/v1/discovery/recommend/songs", &json!({}))
        .await?;
    Ok(extract(&fallback, "/recommend").unwrap_or_default())
}

fn extract(json: &Value, pointer: &str) -> Option<Vec<Song>> {
    let list = json.pointer(pointer)?.as_array()?;
    Some(list.iter().filter_map(Song::from_json).collect())
}

/// 个人漫游（私人 FM）：每次返回一小批（实测 3 首），需要播放端持续补充。
pub async fn radio_get(client: &Client) -> Result<Vec<Song>, ApiError> {
    let json = client.weapi("/weapi/v1/radio/get", &json!({})).await?;
    Ok(extract(&json, "/data").unwrap_or_default())
}

/// 「私人雷达」歌单 id：从推荐资源里按名字找，找不到用固定 id 兜底。
pub async fn private_radar_id(client: &Client) -> u64 {
    if let Ok(json) = client
        .weapi("/weapi/v1/discovery/recommend/resource", &json!({}))
        .await
    {
        if let Some(list) = json.get("recommend").and_then(Value::as_array) {
            if let Some(id) = list
                .iter()
                .find(|item| {
                    item.get("name")
                        .and_then(Value::as_str)
                        .map(|name| name.contains("私人雷达"))
                        .unwrap_or(false)
                })
                .and_then(|item| item.get("id").and_then(Value::as_u64))
            {
                return id;
            }
        }
    }
    3_136_952_023
}
