use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::crypto::{self, WeapiParams};
use crate::error::{ensure_code_ok, ApiError};

pub const BASE_URL: &str = "https://music.163.com";

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

/// 客户端版本号写死成一个「看起来正常」的 PC 客户端值；改它可能触发风控。
const APP_VER: &str = "2.10.12";

/// 无状态调用网易云接口的客户端。
///
/// 自己维护 cookie 表（而不是用 reqwest 的 Jar）：登录要落盘、要能读回，
/// 而且服务端对 `os` / `appver` / `deviceId` 这些字段很敏感，显式控制更稳。
pub struct Client {
    http: reqwest::Client,
    device_id: String,
    cookies: Arc<Mutex<HashMap<String, String>>>,
}

impl Client {
    pub fn new() -> Result<Self, ApiError> {
        if let Ok(device_id) = std::env::var("NCM_DEVICE_ID") {
            return Self::with_device_id(device_id);
        }
        Self::with_device_id(random_device_id())
    }

    pub fn with_device_id(device_id: String) -> Result<Self, ApiError> {
        let mut builder = reqwest::Client::builder().user_agent(USER_AGENT);
        if std::env::var_os("NCM_HTTP1").is_some() {
            builder = builder.http1_only();
        }
        let http = builder.build()?;

        let mut jar = HashMap::new();
        jar.insert("os".to_string(), "pc".to_string());
        jar.insert("appver".to_string(), APP_VER.to_string());
        jar.insert("channel".to_string(), "netease".to_string());
        jar.insert("deviceId".to_string(), device_id.clone());

        Ok(Self {
            http,
            device_id,
            cookies: Arc::new(Mutex::new(jar)),
        })
    }

    /// 当前会话的 __csrf（未登录时为空串）。
    pub fn csrf_token(&self) -> String {
        self.cookies
            .lock()
            .ok()
            .and_then(|cookies| cookies.get("__csrf").cloned())
            .unwrap_or_default()
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// 用外部保存的 cookie（登录后落盘的 MUSIC_U 等）覆盖当前会话。
    pub fn restore_cookies(&self, pairs: impl IntoIterator<Item = (String, String)>) {
        if let Ok(mut cookies) = self.cookies.lock() {
            for (key, value) in pairs {
                cookies.insert(key, value);
            }
        }
    }

    pub fn cookie_pairs(&self) -> Vec<(String, String)> {
        self.cookies
            .lock()
            .map(|cookies| {
                cookies
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 退出登录：清掉鉴权相关 cookie（保留 deviceId/os 等基础指纹）。
    pub fn logout(&self) {
        if let Ok(mut cookies) = self.cookies.lock() {
            cookies.remove("MUSIC_U");
            cookies.remove("MUSIC_A");
            cookies.remove("__csrf");
            cookies.remove("NMTID");
        }
    }

    /// 把会话写到磁盘（登录后调用，下次启动再读回来）。
    pub fn save_cookies(&self, path: &std::path::Path) -> std::io::Result<()> {
        let pairs = self.cookie_pairs();
        let json = serde_json::to_string_pretty(&pairs).map_err(std::io::Error::other)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, json)
    }

    /// 从磁盘恢复会话；文件不存在或损坏时返回 false（当作未登录）。
    pub fn load_cookies(&self, path: &std::path::Path) -> bool {
        let Ok(text) = std::fs::read_to_string(path) else {
            return false;
        };
        let Ok(pairs) = serde_json::from_str::<Vec<(String, String)>>(&text) else {
            return false;
        };
        self.restore_cookies(pairs);
        true
    }

    /// 是否已登录（有 MUSIC_U 或 MUSIC_A）。
    pub fn is_logged_in(&self) -> bool {
        self.cookies
            .lock()
            .map(|cookies| cookies.contains_key("MUSIC_U") || cookies.contains_key("MUSIC_A"))
            .unwrap_or(false)
    }

    fn cookie_header(&self) -> String {
        self.cookies
            .lock()
            .map(|cookies| {
                cookies
                    .iter()
                    .map(|(key, value)| format!("{key}={value}"))
                    .collect::<Vec<_>>()
                    .join("; ")
            })
            .unwrap_or_default()
    }

    fn absorb_set_cookie(&self, response: &reqwest::Response) {
        let mut updates = Vec::new();
        for value in response.headers().get_all(reqwest::header::SET_COOKIE) {
            let Ok(text) = value.to_str() else { continue };
            // Set-Cookie: MUSIC_U=xxx; Path=/; ...
            let Some((pair, _)) = text.split_once(';') else {
                continue;
            };
            let Some((key, val)) = pair.split_once('=') else {
                continue;
            };
            updates.push((key.trim().to_string(), val.trim().to_string()));
        }
        if !updates.is_empty() {
            if let Ok(mut cookies) = self.cookies.lock() {
                for (key, value) in updates {
                    cookies.insert(key, value);
                }
            }
        }
    }

    /// weapi 请求：`path` 形如 `/weapi/search/get`。
    pub async fn weapi(&self, path: &str, data: &Value) -> Result<Value, ApiError> {
        let WeapiParams {
            params,
            enc_sec_key,
        } = crypto::weapi_csrf(data, &self.csrf_token());
        self.post_form(path, &[("params", params), ("encSecKey", enc_sec_key)])
            .await
    }

    /// 同 [`Client::weapi`]，但**不检查**顶层 `code`。
    /// 二维码轮询这类接口用 800/801/802/803 表示业务状态，不能当错误处理。
    pub async fn weapi_raw(&self, path: &str, data: &Value) -> Result<Value, ApiError> {
        let WeapiParams {
            params,
            enc_sec_key,
        } = crypto::weapi_csrf(data, &self.csrf_token());
        self.post_form_raw(path, &[("params", params), ("encSecKey", enc_sec_key)])
            .await
    }

    /// eapi 请求：`path` 传摘要用的路径 `/api/search/get`，
    /// 实际地址会映射成 `/eapi/search/get`（服务端就是这么对应的）。
    pub async fn eapi(&self, path: &str, data: &Value) -> Result<Value, ApiError> {
        let params = crypto::eapi(path, data);
        let url_path = path
            .strip_prefix("/api")
            .map(|rest| format!("/eapi{rest}"))
            .unwrap_or_else(|| path.to_string());
        self.post_form(&url_path, &[("params", params)]).await
    }

    async fn post_form(&self, path: &str, form: &[(&str, String)]) -> Result<Value, ApiError> {
        self.post_form_impl(path, form, true).await
    }

    async fn post_form_raw(&self, path: &str, form: &[(&str, String)]) -> Result<Value, ApiError> {
        self.post_form_impl(path, form, false).await
    }

    async fn post_form_impl(
        &self,
        path: &str,
        form: &[(&str, String)],
        check_code: bool,
    ) -> Result<Value, ApiError> {
        let encoded = form
            .iter()
            .map(|(key, value)| format!("{}={}", urlencode(key), urlencode(value)))
            .collect::<Vec<_>>()
            .join("&");

        let request = self
            .http
            .post(format!("{BASE_URL}{path}"))
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .header(reqwest::header::REFERER, BASE_URL)
            .header(reqwest::header::ORIGIN, BASE_URL)
            .header(reqwest::header::COOKIE, self.cookie_header())
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(encoded)
            .build()?;

        if let Some(path) = std::env::var_os("NCM_DEBUG_BODY") {
            if let Some(bytes) = request.body().and_then(|body| body.as_bytes()) {
                let _ = std::fs::write(path, bytes);
            }
        }

        if std::env::var_os("NCM_DEBUG").is_some() {
            eprintln!("[req] {} {:?}", request.url(), request.headers());
        }

        let response = self.http.execute(request).await?;

        self.absorb_set_cookie(&response);

        let status = response.status().as_u16();
        if std::env::var_os("NCM_DEBUG").is_some() {
            eprintln!(
                "[res] {:?} {status} {:?}",
                response.version(),
                response.headers()
            );
        }
        let text = response.text().await?;
        if std::env::var_os("NCM_DEBUG").is_some() {
            eprintln!("[body] {}", text.chars().take(400).collect::<String>());
        }
        let json: Value = serde_json::from_str(&text).map_err(|_| ApiError::NonJson {
            status,
            body: text.chars().take(160).collect(),
        })?;

        if check_code {
            ensure_code_ok(&json)?;
        }
        Ok(json)
    }
}

/// application/x-www-form-urlencoded 的百分号编码。
fn urlencode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// 16 位 hex 的设备 id（首次启动生成一次，之后持久化复用）。
fn random_device_id() -> String {
    use rand::Rng as _;
    let mut rng = rand::thread_rng();
    (0..16)
        .map(|_| {
            let n: u8 = rng.gen_range(0..16);
            char::from_digit(n as u32, 16).expect("0..16 一定是合法 hex 位")
        })
        .collect()
}

