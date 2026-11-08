//! 音频管线：把一条直链变成能出声的流。
//!
//! 这一层不认识网易云，也不认识 UI：输入是 URL（或已解析好的流），
//! 输出是解码后的采样。播放状态机在 `ncm-core`。

pub mod error;
pub mod stream;

pub use error::AudioError;

use std::path::Path;
use std::time::Duration;

use rodio::Source as _;
use rodio::{Decoder, DeviceSinkBuilder, Player};

/// 音轨的静态信息。用于进度条总长、无设备环境下的解码自检。
#[derive(Debug, Clone)]
pub struct TrackInfo {
    pub duration: Option<Duration>,
    pub sample_rate: u32,
    pub channels: u16,
}

/// 只解码不播放，验证"这条 URL 能不能被正确解出来"。
///
/// CI / 无声卡的机器靠这个做回归：样本率、声道数、时长都是可断言的。
pub async fn probe(url: &str, cache_dir: &Path) -> Result<TrackInfo, AudioError> {
    let source = stream::open(url, cache_dir).await?;
    let decoder = Decoder::new(source)?;

    Ok(TrackInfo {
        duration: decoder.total_duration(),
        sample_rate: decoder.sample_rate().get(),
        channels: decoder.channels().get(),
    })
}

/// 拉流 → 解码 → 播放，阻塞到播完（或到达时长上限）。
///
/// 注意：`MixerDeviceSink` 不是 `Send`，所以这个函数必须在同一个线程里
/// 创建并持有输出设备；不要把它丢进 `tokio::spawn`。
pub async fn play_url(
    url: &str,
    cache_dir: &Path,
    max_duration: Option<Duration>,
) -> Result<(), AudioError> {
    let source = stream::open(url, cache_dir).await?;
    let decoder = Decoder::new(source)?;

    let sink = DeviceSinkBuilder::open_default_sink()
        .map_err(|err| AudioError::Device(err.to_string()))?;
    let player = Player::connect_new(sink.mixer());

    match max_duration {
        Some(limit) => player.append(decoder.take_duration(limit)),
        None => player.append(decoder),
    }
    player.sleep_until_end();

    Ok(())
}
