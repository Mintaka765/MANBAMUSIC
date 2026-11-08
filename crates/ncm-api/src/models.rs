use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 歌曲。只保留播放器真正要用的字段，其余按需再扩。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Song {
    pub id: u64,
    pub name: String,
    pub artists: Vec<String>,
    pub album: String,
    pub album_id: Option<u64>,
    pub cover_url: Option<String>,
    pub duration_ms: u64,
    /// 0=免费 1=VIP 8=低音质免费，见文档 §6.4。
    pub fee: i64,
}

impl Song {
    /// 兼容两种返回结构：
    /// - 搜索/详情：`album` / `artists` / `duration`
    /// - 云搜索/歌单详情：`al` / `ar` / `dt`
    pub fn from_json(value: &Value) -> Option<Self> {
        let id = value.get("id")?.as_u64()?;
        let name = value.get("name")?.as_str()?.to_string();

        let artists_node = value
            .get("artists")
            .or_else(|| value.get("ar"))
            .and_then(Value::as_array);
        let artists = artists_node
            .map(|list| {
                list.iter()
                    .filter_map(|a| a.get("name").and_then(Value::as_str).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();

        let album_node = value.get("album").or_else(|| value.get("al"));
        Some(Self {
            id,
            name,
            artists,
            album: album_node
                .and_then(|a| a.get("name"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            album_id: album_node.and_then(|a| a.get("id")).and_then(Value::as_u64),
            cover_url: album_node
                .and_then(|a| a.get("picUrl"))
                .and_then(Value::as_str)
                .map(str::to_string),
            duration_ms: value
                .get("duration")
                .or_else(|| value.get("dt"))
                .and_then(Value::as_u64)
                .unwrap_or(0),
            fee: value.get("fee").and_then(Value::as_i64).unwrap_or(0),
        })
    }

    /// `歌手 - 歌名`
    pub fn display(&self) -> String {
        if self.artists.is_empty() {
            self.name.clone()
        } else {
            format!("{} - {}", self.artists.join("/"), self.name)
        }
    }
}

/// 播放直链解析结果。`expi_secs` 决定缓存多久（服务端现在给 1200 秒）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayUrl {
    pub url: String,
    pub level: String,
    pub br: u32,
    pub size: u64,
    pub expi_secs: u64,
    /// 网易自带的响度归一化参数（dB），可直接用于音量补偿。
    pub gain: Option<f64>,
    pub peak: Option<f64>,
    /// 非空表示这是**试听片段**（秒数）。未登录听 VIP 歌时通常是 45 秒。
    pub trial_secs: Option<u64>,
}

/// 歌词原始响应。yrc 只有部分曲目提供，必须能回落到 lrc。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RawLyric {
    pub lrc: Option<String>,
    pub yrc: Option<String>,
    pub tlyric: Option<String>,
    pub romalrc: Option<String>,
}

/// 登录后的账号信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub user_id: u64,
    pub nickname: String,
    pub avatar_url: Option<String>,
}

/// 歌单概要。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub id: u64,
    pub name: String,
    pub cover_url: Option<String>,
    pub track_count: u64,
    pub creator_id: Option<u64>,
    /// 官方「我喜欢的音乐」歌单为 5，普通歌单为 0。
    pub special_type: Option<i64>,
}

