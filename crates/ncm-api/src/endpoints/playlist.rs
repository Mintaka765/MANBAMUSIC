//! 歌单写操作：把曲目加入 / 移出歌单。
//!
//! `POST /weapi/playlist/manipulate/tracks`：
//! - `op=add` / `op=del`
//! - `pid` 歌单 id，`trackIds` 是 `"[1,2,3]"` 这样的字符串
//! - `imme=true` 表示立即生效
//!
//! 写接口必须带登录态与 `csrf_token`（`Client::weapi` 自动从 `__csrf` cookie 取）。

use crate::client::Client;
use crate::error::ApiError;
use serde_json::{json, Value};

pub async fn add_tracks(client: &Client, pid: u64, ids: &[u64]) -> Result<(), ApiError> {
    manipulate(client, pid, ids, "add").await
}

pub async fn remove_tracks(client: &Client, pid: u64, ids: &[u64]) -> Result<(), ApiError> {
    manipulate(client, pid, ids, "del").await
}

async fn manipulate(client: &Client, pid: u64, ids: &[u64], op: &str) -> Result<(), ApiError> {
    if ids.is_empty() {
        return Ok(());
    }
    let list = ids
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let data = json!({
        "op": op,
        "pid": pid.to_string(),
        "trackIds": format!("[{list}]"),
        "imme": "true",
    });

    let value = client
        .weapi("/weapi/playlist/manipulate/tracks", &data)
        .await?;
    check_code(&value)
}

/// 成功时可能返回 `code`，也可能只有 `status`；只有出现非 200 的 code 才算失败。
fn check_code(value: &Value) -> Result<(), ApiError> {
    let code = value.get("code").and_then(Value::as_i64).unwrap_or(200);
    if code == 200 {
        return Ok(());
    }
    let msg = value
        .get("msg")
        .or_else(|| value.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("歌单操作失败")
        .to_string();
    Err(ApiError::Api { code, msg })
}
