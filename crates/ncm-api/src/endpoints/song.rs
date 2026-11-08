use crate::client::Client;
use crate::error::ApiError;
use crate::models::{PlayUrl, RawLyric};
use serde_json::{json, Value};

/// 解析播放直链。
///
/// 关键点：外层 `code` 恒为 200，真实结果在 `data[0].code`；
/// 匿名态下 VIP 曲目会得到 `url: null` + 内层 404，必须走降级而不是当作成功。
pub async fn play_url(client: &Client, id: u64, level: &str) -> Result<PlayUrl, ApiError> {
    let encode_type = if matches!(level, "lossless" | "hires") {
        "flac"
    } else {
        "mp3"
    };
    let data = json!({
        "ids": format!("[{id}]"),
        "level": level,
        "encodeType": encode_type,
    });

    let json = client
        .weapi("/weapi/song/enhance/player/url/v1", &data)
        .await?;
    let entry = json.pointer("/data/0").ok_or(ApiError::Shape("data[0]"))?;

    let code = entry.get("code").and_then(Value::as_i64).unwrap_or(-1);
    let fee = entry.get("fee").and_then(Value::as_i64).unwrap_or(0);
    let url = entry
        .get("url")
        .and_then(Value::as_str)
        .map(|u| u.replace("http://", "https://"));

    match url {
        Some(url) if code == 200 => Ok(PlayUrl {
            url,
            level: level.to_string(),
            br: entry.get("br").and_then(Value::as_u64).unwrap_or(0) as u32,
            size: entry.get("size").and_then(Value::as_u64).unwrap_or(0),
            expi_secs: entry.get("expi").and_then(Value::as_u64).unwrap_or(1200),
            gain: entry.get("gain").and_then(Value::as_f64),
            peak: entry.get("peak").and_then(Value::as_f64),
            trial_secs: trial_secs(entry),
        }),
        _ => Err(ApiError::Unplayable {
            message: unplayable_message(code, fee),
            code,
            fee,
        }),
    }
}

/// 把内层 code 翻译成用户能看懂的一句话。
fn unplayable_message(code: i64, fee: i64) -> String {
    match code {
        404 if fee == 1 => "这首歌需要会员或登录后才能播放（当前未登录）".to_string(),
        404 => "该曲目暂不可播放（版权限制或需要登录）".to_string(),
        403 => "请求被拒绝，可能触发了风控，请稍后再试".to_string(),
        _ => format!("曲目不可播放（code={code}）"),
    }
}

/// `freeTrialInfo: { start: 0, end: 45 }`，单位是秒；非试听时为 None。
fn trial_secs(entry: &Value) -> Option<u64> {
    let info = entry.get("freeTrialInfo")?;
    let start = info.get("start").and_then(Value::as_u64).unwrap_or(0);
    let end = info.get("end").and_then(Value::as_u64)?;
    if end > start {
        Some(end - start)
    } else {
        None
    }
}

/// 按偏好顺序逐级降级（lossless → exhigh → standard）。
///
/// 每次降级都会重新请求，不要用字符串拼 URL 猜音质。
pub async fn play_url_with_fallback(
    client: &Client,
    id: u64,
    preferred: &[&str],
) -> Result<PlayUrl, ApiError> {
    let mut last_error = None;

    for level in preferred {
        match play_url(client, id, level).await {
            Ok(resolved) => return Ok(resolved),
            // 不可播放才降级；网络错误直接抛出，避免掩盖真实问题
            Err(err @ ApiError::Unplayable { .. }) => last_error = Some(err),
            Err(other) => return Err(other),
        }
    }

    Err(last_error.unwrap_or(ApiError::Shape("level 阶梯为空")))
}

/// 歌词：lrc / yrc(逐字) / tlyric(翻译) / romalrc(罗马音)。
///
/// 用 `/weapi/song/lyric` 的 `-1` 全开参数：`/song/lyric/v1` 在传 0/false 时
/// 会返回 `{"msg":"参数错误","code":400}`。
pub async fn lyric(client: &Client, id: u64) -> Result<RawLyric, ApiError> {
    let data = json!({
        "id": id,
        "lv": -1,
        "kv": -1,
        "tv": -1,
        "rv": -1,
        "yv": -1,
    });

    let json = client.weapi("/weapi/song/lyric", &data).await?;
    let text = |key: &str| {
        json.pointer(&format!("/{key}/lyric"))
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
    };

    Ok(RawLyric {
        lrc: text("lrc"),
        yrc: text("yrc"),
        tlyric: text("tlyric"),
        romalrc: text("romalrc"),
    })
}
