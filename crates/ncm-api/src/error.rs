use serde_json::Value;

/// 协议层错误。
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("网络请求失败: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON 解析失败: {0}")]
    Json(#[from] serde_json::Error),

    /// 服务端返回了非 JSON（风控、参数错误、限流页面都会这样）。
    #[error("响应不是 JSON (HTTP {status}): {body}")]
    NonJson { status: u16, body: String },

    #[error("接口返回 code={code}: {msg}")]
    Api { code: i64, msg: String },

    /// 外层 code 恒为 200，真实结果在内层 `data[0].code`。
    #[error("{message}")]
    Unplayable {
        message: String,
        code: i64,
        fee: i64,
    },

    #[error("响应结构不符合预期: 缺少 {0}")]
    Shape(&'static str),
}

impl ApiError {
    /// 值得重试的错误：网络抖动、非 JSON 响应、5xx。
    pub fn is_retryable(&self) -> bool {
        matches!(self, ApiError::Http(_) | ApiError::NonJson { .. })
    }
}

pub(crate) fn ensure_code_ok(json: &Value) -> Result<(), ApiError> {
    let code = json.get("code").and_then(Value::as_i64).unwrap_or(200);
    if code == 200 {
        return Ok(());
    }
    let msg = json
        .get("msg")
        .or_else(|| json.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    Err(ApiError::Api { code, msg })
}
