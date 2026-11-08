use crate::client::Client;
use crate::error::ApiError;
use crate::models::{Playlist, Song, UserProfile};
use serde_json::{json, Value};

/// 二维码登录状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QrStatus {
    Expired,
    Waiting,
    Scanned,
    Success,
}

impl QrStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            QrStatus::Expired => "expired",
            QrStatus::Waiting => "waiting",
            QrStatus::Scanned => "scanned",
            QrStatus::Success => "success",
        }
    }
}

/// 二维码类型：3 = 扫码登录（1 是网页登录，App 扫了会提示"无法支持该类型"）。
const QR_TYPE: i64 = 3;

/// 第一步：拿一个二维码 key。二维码内容就是这个 key 本身。
pub async fn qr_unikey(client: &Client) -> Result<String, ApiError> {
    let json = client
        .weapi("/weapi/login/qrcode/unikey", &json!({ "type": QR_TYPE }))
        .await?;
    json.get("unikey")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or(ApiError::Shape("unikey"))
}

/// 第二步：轮询扫码结果。803 时服务端会下发 MUSIC_U 之类的 cookie，
/// 我们的 `Client` 会自动吸收，落盘由上层负责。
pub async fn qr_check(client: &Client, key: &str) -> Result<QrStatus, ApiError> {
    let json = client
        .weapi_raw(
            "/weapi/login/qrcode/client/login",
            &json!({ "key": key, "type": QR_TYPE }),
        )
        .await?;

    let code = json.get("code").and_then(Value::as_i64).unwrap_or(0);
    Ok(match code {
        803 => QrStatus::Success,
        802 => QrStatus::Scanned,
        800 => QrStatus::Expired,
        _ => QrStatus::Waiting,
    })
}

/// 当前账号信息；未登录时 `profile` 为空。
pub async fn account(client: &Client) -> Result<Option<UserProfile>, ApiError> {
    let json = client.weapi("/weapi/nuser/account/get", &json!({})).await?;
    let Some(profile) = json.get("profile") else {
        return Ok(None);
    };
    if profile.is_null() {
        return Ok(None);
    }

    Ok(Some(UserProfile {
        user_id: profile.get("userId").and_then(Value::as_u64).unwrap_or(0),
        nickname: profile
            .get("nickname")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        avatar_url: profile
            .get("avatarUrl")
            .and_then(Value::as_str)
            .map(str::to_string),
    }))
}

/// 某个用户创建/收藏的歌单。
pub async fn user_playlists(client: &Client, uid: u64) -> Result<Vec<Playlist>, ApiError> {
    let json = client
        .weapi(
            "/weapi/user/playlist",
            &json!({ "uid": uid, "limit": 1000, "offset": 0 }),
        )
        .await?;

    let list = json
        .get("playlist")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    Ok(list
        .iter()
        .map(|item| Playlist {
            id: item.get("id").and_then(Value::as_u64).unwrap_or(0),
            name: item
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            cover_url: item
                .get("coverImgUrl")
                .and_then(Value::as_str)
                .map(str::to_string),
            track_count: item.get("trackCount").and_then(Value::as_u64).unwrap_or(0),
            creator_id: item.pointer("/creator/userId").and_then(Value::as_u64),
      special_type: item.get("specialType").and_then(Value::as_i64),
        })
        .filter(|playlist| playlist.id != 0)
        .collect())
}

/// 歌单里的歌。
///
/// detail 接口的 `tracks` 最多只给 1000 首，但 `trackIds` 是全的。
/// 所以先拿 id 列表（`n=0`，很轻），再用 `/weapi/v3/song/detail` 分批换详情。
/// 任一步失败都回落到一次性 detail，保证小歌单不受影响。
pub async fn playlist_tracks(client: &Client, id: u64) -> Result<Vec<Song>, ApiError> {
    const BATCH: usize = 400;
    const MAX_TRACKS: usize = 5000;

    let meta = client
        .weapi("/weapi/v6/playlist/detail", &json!({ "id": id, "n": 0, "s": 0 }))
        .await;

    let ids: Vec<u64> = meta
        .ok()
        .and_then(|value| {
            value
                .pointer("/playlist/trackIds")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.get("id").and_then(Value::as_u64))
                        .take(MAX_TRACKS)
                        .collect::<Vec<_>>()
                })
        })
        .unwrap_or_default();

    if ids.len() <= 1000 {
        return playlist_tracks_detail(client, id).await;
    }

    let mut songs: Vec<Song> = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(BATCH) {
        let c = chunk
            .iter()
            .map(|id| format!("{{\"id\":{id}}}"))
            .collect::<Vec<_>>()
            .join(",");
        let page = client
            .weapi("/weapi/v3/song/detail", &json!({ "c": format!("[{c}]") }))
            .await;

        let Ok(page) = page else {
            break;
        };
        let batch = page
            .get("songs")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        songs.extend(batch.iter().filter_map(Song::from_json));
    }

    if songs.len() < ids.len() {
        // 有批次失败：仍然按 id 顺序补齐能拿到的部分
        let mut by_id: std::collections::HashMap<u64, Song> =
            songs.into_iter().map(|song| (song.id, song)).collect();
        let ordered: Vec<Song> = ids.iter().filter_map(|id| by_id.remove(id)).collect();
        if ordered.is_empty() {
            return playlist_tracks_detail(client, id).await;
        }
        return Ok(ordered);
    }

    // 按 trackIds 顺序重排（song/detail 不保证顺序）
    let mut by_id: std::collections::HashMap<u64, Song> =
        songs.into_iter().map(|song| (song.id, song)).collect();
    Ok(ids.iter().filter_map(|id| by_id.remove(id)).collect())
}

async fn playlist_tracks_detail(client: &Client, id: u64) -> Result<Vec<Song>, ApiError> {
    let json = client
        .weapi(
            "/weapi/v6/playlist/detail",
            &json!({ "id": id, "n": 1000, "s": 8 }),
        )
        .await?;

    let tracks = json
        .pointer("/playlist/tracks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    Ok(tracks.iter().filter_map(Song::from_json).collect())
}

