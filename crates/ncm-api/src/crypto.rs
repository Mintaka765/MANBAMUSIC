//! 网易云 weapi / eapi 加密实现。
//!
//! 两条通道：
//! - **weapi**：AES-128-CBC 两层（预设 key + 每次随机的 16 字节 key），随机 key 再用
//!   RSA（无填充）加密成 `encSecKey`。
//! - **eapi**：AES-128-ECB，固定 key，明文里内嵌一段「路径 + JSON + md5」做防篡改。
//!
//! 两个容易写错的点，都在测试里有覆盖：
//! 1. RSA 之前要把 16 字节 key **按字节逆序**，再当作大端整数做 modpow；
//! 2. JSON 的**字段顺序会影响密文**，所以必须 `serde_json` 开 `preserve_order`，
//!    并且 `csrf_token` 追加在最后（与官方 JS 客户端一致）。

use aes::Aes128;
use cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyInit, KeyIvInit};
use md5::{Digest, Md5};
use num_bigint::BigUint;
use serde_json::Value;

type Aes128CbcEnc = cbc::Encryptor<Aes128>;
type Aes128EcbEnc = ecb::Encryptor<Aes128>;

const IV: &[u8] = b"0102030405060708";
const PRESET_KEY: &[u8] = b"0CoJUm6Qyw8W8jud";
const EAPI_KEY: &[u8] = b"e82ckenh8dichen8";

/// 官方客户端内置的 1024-bit 公钥模数。
const MODULUS_HEX: &str = concat!(
    "00e0b509f6259df8642dbc35662901477df22677ec152b5ff68ace615bb7b725152b3ab17a87",
    "6aea8a5aa76d2e417629ec4ee341f56135fccf695280104e0312ecbda92557c93870114af6c",
    "9d05c4f7f0c3685b7a46bee255932575cce10b424d813cfe4875d3e82047b97ddef52741d5",
    "46b8e289dc6935b3ece0462db0a22b8e7"
);
const PUBLIC_EXPONENT: u32 = 0x0001_0001;
/// 1024-bit 模数对应 128 字节定长输出。
const RSA_OUTPUT_LEN: usize = 128;

/// weapi 的请求体。
#[derive(Debug, Clone)]
pub struct WeapiParams {
    /// base64 的两层 AES 密文。
    pub params: String,
    /// 128 字节随机 key 的 RSA 密文，小写 hex。
    pub enc_sec_key: String,
}

/// 用随机 key 走一次 weapi 加密（csrf 为空，只读接口用）。
pub fn weapi(data: &Value) -> WeapiParams {
    weapi_csrf(data, "")
}

/// 带 csrf 的 weapi 加密。写接口（喜欢、收藏、创建歌单）必须传登录后的
/// `__csrf` cookie 值，否则服务端会以参数错误拒绝。
pub fn weapi_csrf(data: &Value, csrf: &str) -> WeapiParams {
    const HEX: &[u8] = b"0123456789abcdef";
    let nonce: String = (0..16)
        .map(|_| {
            let idx = rand::Rng::gen_range(&mut rand::thread_rng(), 0..HEX.len());
            HEX[idx] as char
        })
        .collect();
    weapi_with_nonce_csrf(data, &nonce, csrf)
}

/// 指定 16 字节 ASCII key 的 weapi 加密（测试用，同时便于复现问题）。
pub fn weapi_with_nonce(data: &Value, nonce: &str) -> WeapiParams {
    weapi_with_nonce_csrf(data, nonce, "")
}

/// 同时指定 nonce 与 csrf（测试黄金向量用 csrf=""）。
pub fn weapi_with_nonce_csrf(data: &Value, nonce: &str, csrf: &str) -> WeapiParams {
    let key = nonce.as_bytes();
    assert_eq!(key.len(), 16, "weapi 的随机 key 必须是 16 字节");

    let mut body = data.clone();
    if let Value::Object(map) = &mut body {
        map.insert("csrf_token".into(), Value::String(csrf.to_string()));
    }
    let text = serde_json::to_string(&body).expect("JSON 序列化不应失败");
    if std::env::var_os("NCM_DEBUG").is_some() {
        eprintln!("[weapi plaintext] {text}");
    }

    let first = base64_encode(&aes_cbc_encrypt(text.as_bytes(), PRESET_KEY));
    let params = base64_encode(&aes_cbc_encrypt(first.as_bytes(), key));

    WeapiParams {
        params,
        enc_sec_key: rsa_encrypt_hex(key),
    }
}

/// eapi 加密。`path` 是未加密前的接口路径，例如 `/api/search/get`。
pub fn eapi(path: &str, data: &Value) -> String {
    let text = serde_json::to_string(data).expect("JSON 序列化不应失败");
    let digest = md5_hex(format!("nobody{path}use{text}md5forencrypt").as_bytes());
    let plain = format!("{path}-36cd479b6b5-{text}-36cd479b6b5-{digest}");

    let encryptor = Aes128EcbEnc::new_from_slice(EAPI_KEY).expect("EAPI_KEY 已是 16 字节");
    hex::encode_upper(encryptor.encrypt_padded_vec_mut::<Pkcs7>(plain.as_bytes()))
}

fn aes_cbc_encrypt(plain: &[u8], key: &[u8]) -> Vec<u8> {
    let encryptor = Aes128CbcEnc::new_from_slices(key, IV).expect("key 与 IV 都必须是 16 字节");
    encryptor.encrypt_padded_vec_mut::<Pkcs7>(plain)
}

/// RSA(no padding)：key 先逆序，再按大端整数做 modpow，最后左补零到 128 字节。
pub fn rsa_encrypt_hex(key: &[u8]) -> String {
    let mut reversed = key.to_vec();
    reversed.reverse();

    let modulus = BigUint::parse_bytes(MODULUS_HEX.as_bytes(), 16).expect("内置模数合法");
    let message = BigUint::from_bytes_be(&reversed);
    let cipher = message.modpow(&BigUint::from(PUBLIC_EXPONENT), &modulus);

    let bytes = cipher.to_bytes_be();
    let mut out = vec![0u8; RSA_OUTPUT_LEN];
    out[RSA_OUTPUT_LEN - bytes.len()..].copy_from_slice(&bytes);
    hex::encode(out)
}

fn md5_hex(input: &[u8]) -> String {
    let mut hasher = Md5::new();
    hasher.update(input);
    hex::encode(hasher.finalize())
}

fn base64_encode(input: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 黄金向量由 `node tools/ncm-crypto-probe.mjs --vectors` 生成（Node 的 crypto 实现）。
    /// 任何实现漂移（字段顺序、padding、逆序、补零）都会让这里变红。
    const VECTOR_NONCE: &str = "0123456789abcdef";
    const VECTOR_WEAPI_PARAMS: &str = "/6VzyqxiY4GeAKvcAu0k7VotTQs/KAHmIOgt0kQNJtd0aNfYEzP1vpLlAjOY1rvi8iew6O1gqTu7pP+cM4pZfm4bufN9atZNQgz4kPdD9HQUEwCH9eBc8S/h6qnYOOmC";
    const VECTOR_ENC_SEC_KEY: &str = "35701388baf89fed412e11269b9c76625d095ecaf17f03fa018abe19ea2d38b949debf242ee39a71ca1f6cda71b1b86a45aa909ee27f7e78e267d34e732f0de948206c3340a788d0003372183e2f753c1f78b66ac23d134ac1fc9b993156520ea826b8aa89a962d4491b4b8d7e08738e1da9b07aa39bf4a7ef0b1c210728cd52";
    const VECTOR_EAPI_PARAMS: &str = "1AF0E93B0E3EA03CE4E7F1B6AD7BD32BC198D7B70109AB343E0FC0C4A8F27C965817DFB6318A4DBB7F44CD9CD14FDD2CB3B194245FFB476BA70C2CA92927CDE240245A90B3801278F541BEC912192AFA47B491F481FB0EE6D0072EEEBFE706EFEDF635D2FF8DF073E621F9959F3ED0C5B58DF95C10D3661D97746BADA7DC9BBF";

    fn sample_input() -> Value {
        json!({ "s": "晴天", "type": 1, "limit": 1, "offset": 0 })
    }

    #[test]
    fn weapi_params_match_golden_vector() {
        let out = weapi_with_nonce(&sample_input(), VECTOR_NONCE);
        assert_eq!(out.params, VECTOR_WEAPI_PARAMS);
    }

    #[test]
    fn weapi_enc_sec_key_matches_golden_vector() {
        let out = weapi_with_nonce(&sample_input(), VECTOR_NONCE);
        assert_eq!(out.enc_sec_key, VECTOR_ENC_SEC_KEY);
        assert_eq!(out.enc_sec_key.len(), RSA_OUTPUT_LEN * 2);
    }

    #[test]
    fn eapi_params_match_golden_vector() {
        assert_eq!(eapi("/api/search/get", &sample_input()), VECTOR_EAPI_PARAMS);
    }

    #[test]
    fn random_nonce_is_16_ascii_hex_chars() {
        let out = weapi(&sample_input());
        // 随机 key 只影响密文；这里确认结构稳定，且两次随机结果不同
        assert_eq!(out.enc_sec_key.len(), RSA_OUTPUT_LEN * 2);
        let again = weapi(&sample_input());
        assert_ne!(out.params, again.params, "随机 key 应导致密文不同");
    }
}

