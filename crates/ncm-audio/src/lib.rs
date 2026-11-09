//! 音频管线：把一条直链变成能出声的流。
//!
//! 这一层不认识网易云，也不认识 UI：输入是 URL（或已解析好的流），
//! 输出是解码后的采样。播放状态机在 `ncm-core`。

pub mod error;
pub mod progressive;
pub mod stream;

pub use error::AudioError;
pub use progressive::ProgressiveCache;

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

// ---------------------------------------------------------------------------
// 本地缓存：整首下完后解码器读的是可随机访问的文件，前后拖都是瞬时的
// ---------------------------------------------------------------------------

/// 某首歌的缓存文件路径（扩展名统一用 .audio，解码器按内容识别格式）。
pub fn cache_file(cache_dir: &Path, key: &str) -> std::path::PathBuf {
    cache_dir.join("tracks").join(format!("{key}.audio"))
}

/// 整首下载到缓存。已存在且非空直接返回（幂等）。
///
/// 先写 `.part` 再改名，避免中途失败留下半截文件被当成完整缓存。
pub async fn download_to_cache(url: &str, dest: &Path) -> Result<(), AudioError> {
    if let Ok(meta) = std::fs::metadata(dest) {
        if meta.len() > 0 {
            return Ok(());
        }
    }

    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir).map_err(|err| AudioError::Stream(err.to_string()))?;
    }

    let response = reqwest::get(url)
        .await
        .map_err(|err| AudioError::Stream(err.to_string()))?
        .error_for_status()
        .map_err(|err| AudioError::Stream(err.to_string()))?;
    let bytes = response
        .bytes()
        .await
        .map_err(|err| AudioError::Stream(err.to_string()))?;
    if bytes.is_empty() {
        return Err(AudioError::Stream("缓存下载得到空响应".into()));
    }

    let partial = dest.with_extension("part");
    std::fs::write(&partial, &bytes).map_err(|err| AudioError::Stream(err.to_string()))?;
    std::fs::rename(&partial, dest).map_err(|err| AudioError::Stream(err.to_string()))?;
    Ok(())
}

/// 打开本地缓存文件（可 Seek，symphonia 能随机访问）。
pub fn decoder_seekable<R>(data: R, byte_len: u64) -> Result<Decoder<R>, AudioError>
where
    R: std::io::Read + std::io::Seek + Send + Sync + 'static,
{
    Decoder::builder()
        .with_data(data)
        .with_byte_len(byte_len)
        .build()
        .map_err(AudioError::Decode)
}

pub fn open_cached(path: &Path) -> Result<Decoder<std::io::BufReader<std::fs::File>>, AudioError> {
    let file = std::fs::File::open(path).map_err(|err| AudioError::Stream(err.to_string()))?;
    let len = file
        .metadata()
        .map_err(|err| AudioError::Stream(err.to_string()))?
        .len();
    decoder_seekable(std::io::BufReader::new(file), len)
}

/// 缓存上限：1 GB 或 150 首，先到哪个算哪个。
pub const CACHE_MAX_BYTES: u64 = 1024 * 1024 * 1024;
pub const CACHE_MAX_FILES: usize = 150;

/// 播放命中缓存时更新 mtime，淘汰按"最久没听"排序。
pub fn touch_cache(path: &Path) {
    if let Ok(file) = std::fs::OpenOptions::new().write(true).open(path) {
        let _ = file.set_modified(std::time::SystemTime::now());
    }
}

/// 按上限淘汰缓存：保留最近访问的，直到数量与体积都不超限。
pub fn prune_cache(cache_dir: &Path) {
    prune_cache_with(cache_dir, CACHE_MAX_BYTES, CACHE_MAX_FILES);
}

pub fn prune_cache_with(cache_dir: &Path, max_bytes: u64, max_files: usize) {
    let dir = cache_dir.join("tracks");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };

    // (最后访问时间, 大小, 路径)
    let mut items: Vec<(std::time::SystemTime, u64, std::path::PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("audio") {
                return None;
            }
            let meta = entry.metadata().ok()?;
            let modified = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
            Some((modified, meta.len(), path))
        })
        .collect();

    // 新的在前
    items.sort_by_key(|item| std::cmp::Reverse(item.0));

    let mut total = 0u64;
    for (index, (_, size, path)) in items.iter().enumerate() {
        let over_count = index >= max_files;
        let over_bytes = total + size > max_bytes;
        if over_count || over_bytes {
            let _ = std::fs::remove_file(path);
        } else {
            total += size;
        }
    }
}

/// 打开本地文件用于播放：走可随机访问的读取端，前后拖都可用。
pub fn open_local(path: &Path) -> Result<Decoder<progressive::CacheReader>, AudioError> {
    let reader = progressive::ProgressiveCache::open_complete(path)?;
    let len = reader.total();
    decoder_seekable(reader, len)
}


#[cfg(test)]
mod tests {
    use super::*;

    fn make_file(dir: &Path, name: &str, size: u64) {
        let file = std::fs::File::create(dir.join(name)).expect("创建测试文件");
        file.set_len(size).expect("设置长度");
    }

    #[test]
    fn prune_keeps_within_limits() {
        let root = std::env::temp_dir().join(format!("mbm-cache-test-{}", std::process::id()));
        let tracks = root.join("tracks");
        std::fs::create_dir_all(&tracks).expect("建目录");

        for index in 0..10 {
            make_file(&tracks, &format!("{index}.audio"), 1024);
        }

        // 只允许 4 个文件、3 KB
        prune_cache_with(&root, 3 * 1024, 4);

        let left: Vec<_> = std::fs::read_dir(&tracks).expect("读目录").filter_map(Result::ok).collect();
        assert!(left.len() <= 4, "文件数应被裁到 4，实际 {}", left.len());
        let total: u64 = left.iter().filter_map(|entry| entry.metadata().ok()).map(|meta| meta.len()).sum();
        assert!(total <= 3 * 1024, "总大小应不超过 3KB，实际 {total}");

        let _ = std::fs::remove_dir_all(&root);
    }
}


