use crate::client::Client;
use crate::error::ApiError;
use serde_json::{json, Value};
use std::collections::HashSet;

/// 喜欢 / 取消喜欢一首歌。
///
/// 网易仍在用 `/weapi/radio/like` 作为写接口（客户端「红心」就是它）：
/// `alg=itembased` 表示按曲目维度，`time=3` 是固定值，`like` 是字符串。
/// 写接口必须带登录后的 `csrf_token`，由 [`Client::weapi`] 自动从 cookie 取。
pub async fn like(client: &Client, id: u64, like: bool) -> Result<(), ApiError> {
    let data = json!({
        "alg": "itembased",
        "trackId": id.to_string(),
        "like": if like { "true" } else { "false" },
        "time": "3",
    });
    client.weapi("/weapi/radio/like", &data).await?;
    Ok(())
}

/// 批量查询「是否已喜欢」。
///
/// 接口不同时期返回过两种结构：`{ songs: [...] }` 与 `{ data: { songs: [...] } }`，
/// 元素里可能用 `liked`/`likedCount` 或直接出现在 `ids` 数组中。
/// 解析失败时返回空集合——红心只是装饰，不该拖垮列表加载。
pub async fn liked_ids(client: &Client, ids: &[u64]) -> Result<HashSet<u64>, ApiError> {
    if ids.is_empty() {
        return Ok(HashSet::new());
    }
    let list = ids
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let data = json!({
        "trackIds": format!("[{list}]"),
    });

    let value = client.weapi("/weapi/song/like/check", &data).await?;

    Ok(parse_liked(&value))
}

fn parse_liked(value: &Value) -> HashSet<u64> {
    let mut liked = HashSet::new();

    // 形态一：{ "songs": [ { "id": 1, "liked": true } ] }
    // 形态二：{ "data": { "songs": [...] } }
    let songs = value
        .get("songs")
        .or_else(|| value.pointer("/data/songs"))
        .and_then(Value::as_array);

    if let Some(songs) = songs {
        for song in songs {
            let id = song.get("id").and_then(Value::as_u64);
            let flag = song
                .get("liked")
                .and_then(Value::as_bool)
                .or_else(|| song.get("liked").and_then(Value::as_i64).map(|v| v != 0))
                .unwrap_or(true);
            if let (Some(id), true) = (id, flag) {
                liked.insert(id);
            }
        }
    }

    // 形态三：{ "ids": [1, 2], "code": 200 }
    // 实测 /weapi/song/like/check 返回的就是这个结构（顶层 ids 即已喜欢的曲目）。
    let plain = value
        .get("ids")
        .or_else(|| value.pointer("/data/ids"))
        .and_then(Value::as_array);
    if let Some(items) = plain {
        for item in items {
            if let Some(id) = item
                .get("id")
                .and_then(Value::as_u64)
                .or_else(|| item.as_u64())
            {
                liked.insert(id);
            }
        }
    }

    liked
}


