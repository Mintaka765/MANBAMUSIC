//! 渐进缓存：边下边播，已下载的部分可以随意前后拖。
//!
//! 官方客户端就是这个体验：缓存到哪就能拖到哪，超出缓存范围只是短暂等一下。
//! 做法是后台把整首顺序写进一个文件，播放端给它一个"总长已知、读取会等待"
//! 的 Read + Seek 视图，解码器于是能随机访问（symphonia 的要求是 Seek 且长度已知）。

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use crate::error::AudioError;

struct Progress {
    downloaded: Mutex<u64>,
    done: AtomicBool,
    failed: Mutex<Option<String>>,
    signal: Condvar,
}

impl Progress {
    fn new() -> Self {
        Self {
            downloaded: Mutex::new(0),
            done: AtomicBool::new(false),
            failed: Mutex::new(None),
            signal: Condvar::new(),
        }
    }

    fn finish(&self, error: Option<String>) {
        if let Some(message) = error {
            if let Ok(mut slot) = self.failed.lock() {
                *slot = Some(message);
            }
        }
        self.done.store(true, Ordering::SeqCst);
        self.signal.notify_all();
    }
}

/// 一首歌的渐进缓存句柄。
pub struct ProgressiveCache {
    path: PathBuf,
    total: u64,
    state: Arc<Progress>,
}

impl ProgressiveCache {
    /// 打开/启动缓存。文件已完整则直接返回（reader 立即可用）。
    pub async fn start(url: &str, dest: &Path) -> Result<Self, AudioError> {
        if let Some(dir) = dest.parent() {
            std::fs::create_dir_all(dir).map_err(|err| AudioError::Stream(err.to_string()))?;
        }

        let partial = dest.with_extension("part");
        if let Ok(meta) = std::fs::metadata(&partial) {
            if meta.len() > 0 {
                // 已经完整（.part 改名前）理论上不会走到，保险起见按完成处理
                let total = meta.len();
                let state = Arc::new(Progress::new());
                if let Ok(mut downloaded) = state.downloaded.lock() {
                    *downloaded = total;
                }
                state.finish(None);
                return Ok(Self {
                    path: partial,
                    total,
                    state,
                });
            }
        }

        let response = reqwest::Client::new()
            .get(url)
            .send()
            .await
            .map_err(|err| AudioError::Stream(err.to_string()))?
            .error_for_status()
            .map_err(|err| AudioError::Stream(err.to_string()))?;

        let total = response.content_length().ok_or_else(|| {
            AudioError::Stream("服务端没给 Content-Length，无法渐进缓存".into())
        })?;
        if total == 0 {
            return Err(AudioError::Stream("响应长度为 0".into()));
        }

        // 先建空文件，读端按总长等待数据
        std::fs::File::create(&partial).map_err(|err| AudioError::Stream(err.to_string()))?;

        let state = Arc::new(Progress::new());
        let write_path = partial.clone();
        let write_state = state.clone();

        tokio::spawn(async move {
            use std::io::Write;

            let mut response = response;
            let mut file = match std::fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&write_path)
            {
                Ok(file) => file,
                Err(err) => {
                    write_state.finish(Some(err.to_string()));
                    return;
                }
            };

            let mut written: u64 = 0;
            loop {
                match response.chunk().await {
                    Ok(Some(chunk)) => {
                        if let Err(err) = file.write_all(&chunk) {
                            write_state.finish(Some(err.to_string()));
                            return;
                        }
                        written += chunk.len() as u64;
                        if let Ok(mut downloaded) = write_state.downloaded.lock() {
                            *downloaded = written;
                        }
                        write_state.signal.notify_all();
                    }
                    Ok(None) => break,
                    Err(err) => {
                        write_state.finish(Some(err.to_string()));
                        return;
                    }
                }
            }

            if let Err(err) = file.flush() {
                write_state.finish(Some(err.to_string()));
                return;
            }
            write_state.finish(None);
        });

        Ok(Self {
            path: partial,
            total,
            state,
        })
    }

    /// 造一个"读到未下载区域就等"的随机访问视图。
    pub fn reader(&self) -> Result<CacheReader, AudioError> {
        let file = std::fs::File::open(&self.path)
            .map_err(|err| AudioError::Stream(err.to_string()))?;
        Ok(CacheReader {
            file,
            pos: 0,
            total: self.total,
            state: self.state.clone(),
        })
    }

    /// 全部下完后把 .part 改名成正式缓存文件。
    pub fn finalize(&self) -> Result<(), AudioError> {
        let final_path = self.path.with_extension("audio");
        std::fs::rename(&self.path, &final_path).map_err(|err| AudioError::Stream(err.to_string()))
    }
}

/// 可随机访问的读取端：读尚未下载的区域会阻塞等待。
pub struct CacheReader {
    file: std::fs::File,
    pos: u64,
    total: u64,
    state: Arc<Progress>,
}

impl CacheReader {
    fn wait_for(&self, need: u64) -> std::io::Result<()> {
        loop {
            let downloaded = self
                .state
                .downloaded
                .lock()
                .map(|value| *value)
                .unwrap_or(0);
            if downloaded >= need || self.state.done.load(Ordering::SeqCst) {
                if downloaded < need {
                    if let Some(message) = self
                        .state
                        .failed
                        .lock()
                        .ok()
                        .and_then(|slot| slot.clone())
                    {
                        return Err(std::io::Error::other(message));
                    }
                }
                return Ok(());
            }

            if let Ok(guard) = self.state.downloaded.lock() {
                let _ = self
                    .state
                    .signal
                    .wait_timeout(guard, Duration::from_millis(100));
            }
        }
    }
}

impl Read for CacheReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.total || buf.is_empty() {
            return Ok(0);
        }
        self.wait_for(self.pos + 1)?;

        let remaining = (self.total - self.pos).min(buf.len() as u64) as usize;
        self.file.seek(SeekFrom::Start(self.pos))?;
        let read = self.file.read(&mut buf[..remaining])?;
        self.pos += read as u64;
        Ok(read)
    }
}

impl Seek for CacheReader {
    fn seek(&mut self, from: SeekFrom) -> std::io::Result<u64> {
        let next = match from {
            SeekFrom::Start(offset) => offset as i128,
            SeekFrom::End(offset) => self.total as i128 + offset as i128,
            SeekFrom::Current(offset) => self.pos as i128 + offset as i128,
        };
        if next < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "seek 到负数位置",
            ));
        }
        self.pos = next as u64;
        Ok(self.pos)
    }
}

impl ProgressiveCache {
    /// 等下载完成后把 .part 改名成正式缓存文件（后台线程，不阻塞音频线程）。
    pub fn spawn_finalize(self) {
        std::thread::spawn(move || loop {
            if self.state.done.load(Ordering::SeqCst) {
                let failed = self
                    .state
                    .failed
                    .lock()
                    .ok()
                    .and_then(|slot| slot.clone());
                if failed.is_none() {
                    let _ = self.finalize();
                }
                return;
            }
            std::thread::sleep(Duration::from_millis(300));
        });
    }
}