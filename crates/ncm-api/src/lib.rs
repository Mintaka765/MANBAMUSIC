//! 网易云音乐协议层。
//!
//! 这一层只做「把请求发出去、把响应解析成结构体」：
//! - 加密见 [`crypto`]；
//! - 会话与请求头见 [`client`]；
//! - 业务语义（降级、缓存、重试）属于上层 `ncm-core`，不要塞进来。

pub mod client;
pub mod crypto;
pub mod endpoints;
pub mod error;
pub mod models;

pub use client::Client;
pub use error::ApiError;
pub use models::{PlayUrl, Playlist, RawLyric, Song, UserProfile};
