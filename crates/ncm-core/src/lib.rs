//! 业务中枢：播放器 actor。
//!
//! 一个独占音频设备的线程持有 `MixerDeviceSink`（它不是 `Send`），外部只通过
//! 命令通道操作它，通过快照读状态。这样 UI 永远不会碰音频对象，也就不会
//! 因为锁竞争把界面卡住。
//!
//! 无缝切歌的做法：下一首提前 `Enqueue` 进 rodio 的播放队列，当前这一首放完
//! 后混音器直接接着出下一首，中间没有"停 → 等网络 → 再开"的空档。每首源都被
//! [`TrackTap`] 包一层，在真正读完时上报 `TrackEnded`，所以即使队列里还排着
//! 下一首，上层也能准确知道"刚才那首结束了"。

use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rodio::source::SeekError;
use rodio::{
    ChannelCount, Decoder, DeviceSinkBuilder, Player as RodioPlayer, Sample, SampleRate, Source,
};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

#[derive(Debug, Clone)]
pub enum Command {
    /// 立刻切到这一首（会清空已排队的后续曲目）。
    Play {
        url: String,
        track_id: u64,
        /// 非空表示服务端只给了试听片段（秒）。
        trial_secs: Option<u64>,
    },
    /// 排到队尾，用于预挂载下一首。
    Enqueue {
        url: String,
        track_id: u64,
    },
    Pause,
    Resume,
    Stop,
    Seek(u64),
    SetVolume(f32),
}

/// 播到自然结束（用于队列自动切下一首）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    TrackEnded { track_id: u64 },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PlaybackState {
    #[default]
    Idle,
    Loading,
    Playing,
    Paused,
    Error,
}

impl PlaybackState {
    pub fn as_str(self) -> &'static str {
        match self {
            PlaybackState::Idle => "idle",
            PlaybackState::Loading => "loading",
            PlaybackState::Playing => "playing",
            PlaybackState::Paused => "paused",
            PlaybackState::Error => "error",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub state: PlaybackState,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub volume: f32,
    pub error: Option<String>,
    /// 当前正在播的曲目 id（预挂载下一首时不会变）。
    pub track_id: Option<u64>,
    /// 非空表示当前是试听片段。
    pub trial_secs: Option<u64>,
}

pub struct Player {
    tx: Sender<Command>,
    snapshot: Arc<Mutex<Snapshot>>,
}

impl Player {
    /// 起一条音频线程。`runtime` 用来创建 HTTP 流（下载任务跑在它的 worker 上），
    /// `cache_dir` 是流缓冲落盘的位置。
    pub fn spawn(runtime: Arc<Runtime>, cache_dir: PathBuf) -> (Self, UnboundedReceiver<Event>) {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (event_tx, event_rx) = unbounded_channel();
        let snapshot = Arc::new(Mutex::new(Snapshot {
            volume: 0.8,
            ..Default::default()
        }));

        let shared = snapshot.clone();
        // 启动时先裁一次缓存（上限 1GB / 150 首），防止上次异常退出留下超额缓存
        ncm_audio::prune_cache(&cache_dir);
        std::thread::Builder::new()
            .name("ncm-player".into())
            .spawn(move || audio_loop(cmd_rx, shared, runtime, cache_dir, event_tx))
            .expect("创建音频线程失败");

        (
            Self {
                tx: cmd_tx,
                snapshot,
            },
            event_rx,
        )
    }

    pub fn send(&self, command: Command) {
        let _ = self.tx.send(command);
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

/// 在源真正读完时上报一次 `TrackEnded`，用于队列推进。
struct TrackTap<S> {
    inner: S,
    track_id: u64,
    events: UnboundedSender<Event>,
    reported: bool,
}

impl<S> TrackTap<S> {
    fn new(inner: S, track_id: u64, events: UnboundedSender<Event>) -> Self {
        Self {
            inner,
            track_id,
            events,
            reported: false,
        }
    }
}

impl<S: Source> Iterator for TrackTap<S> {
    type Item = Sample;

    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.inner.next();
        if sample.is_none() && !self.reported {
            self.reported = true;
            let _ = self.events.send(Event::TrackEnded {
                track_id: self.track_id,
            });
        }
        sample
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<S: Source> Source for TrackTap<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    fn channels(&self) -> ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.inner.try_seek(pos)
    }
}

fn audio_loop(
    rx: mpsc::Receiver<Command>,
    snapshot: Arc<Mutex<Snapshot>>,
    runtime: Arc<Runtime>,
    cache_dir: PathBuf,
    events: UnboundedSender<Event>,
) {
    let update = |f: &dyn Fn(&mut Snapshot)| {
        if let Ok(mut guard) = snapshot.lock() {
            f(&mut guard);
        }
    };

    // 设备必须在当前线程创建并持有：MixerDeviceSink 内含 cpal::Stream，不是 Send。
    let sink = match DeviceSinkBuilder::open_default_sink() {
        Ok(sink) => sink,
        Err(err) => {
            update(&|s| {
                s.state = PlaybackState::Error;
                s.error = Some(format!("没有可用的音频输出设备: {err}"));
            });
            return;
        }
    };

    let mut player = RodioPlayer::connect_new(sink.mixer());
    if let Ok(volume) = snapshot.lock().map(|s| s.volume) {
        player.set_volume(volume);
    }

    let mut duration_ms = 0u64;
    // 记住当前曲目的直链：反向 seek 不被 symphonia 支持时要重开流
    let mut current: Option<(String, u64)> = None;
    // 重开流后 get_pos() 从 0 开始，用它补回真实位置
    let mut base_ms: u64 = 0;
    // 当前曲目的本地缓存文件（存在即表示整首已下完、可随机访问）
    let mut cache_file: Option<std::path::PathBuf> = None;

    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Command::Play {
                url,
                track_id,
                trial_secs,
            }) => {
                update(&|s| {
                    s.state = PlaybackState::Loading;
                    s.error = None;
                    s.position_ms = 0;
                    s.duration_ms = 0;
                    s.track_id = Some(track_id);
                    s.trial_secs = trial_secs;
                });
                duration_ms = 0;
                // 记住直链：反向 seek 不被支持时要用它重开流
                current = Some((url.clone(), track_id));
                base_ms = 0;

                // 整首缓存：存在就直接读本地文件（可随机访问，前后拖都瞬时）
                let cached = ncm_audio::cache_file(&cache_dir, &track_id.to_string());
                cache_file = Some(cached.clone());
                if cached.exists() {
                    ncm_audio::touch_cache(&cached);
                    match ncm_audio::open_cached(&cached) {
                        Ok(decoder) => {
                            duration_ms = decoder
                                .total_duration()
                                .map(|d| d.as_millis() as u64)
                                .unwrap_or(0);
                            player.stop();
                            player.append(TrackTap::new(decoder, track_id, events.clone()));
                            player.play();
                            update(&|s| {
                                s.state = PlaybackState::Playing;
                                s.duration_ms = duration_ms;
                            });
                            continue;
                        }
                        Err(err) => {
                            // 缓存文件坏了：删掉重走流式
                            eprintln!("缓存损坏，重新下载（track {track_id}）: {err}");
                            let _ = std::fs::remove_file(&cached);
                        }
                    }
                }

                // 渐进缓存：边下边播，已下载的部分可随意前后拖（官方客户端的手感）
                let progressive =
                    runtime.block_on(ncm_audio::ProgressiveCache::start(&url, &cached));
                match progressive {
                    Ok(cache) => {
                        match cache.reader().map(Decoder::new) {
                            Ok(Ok(decoder)) => {
                                duration_ms = decoder
                                    .total_duration()
                                    .map(|d| d.as_millis() as u64)
                                    .unwrap_or(0);
                                player.stop();
                                player.append(TrackTap::new(
                                    decoder,
                                    track_id,
                                    events.clone(),
                                ));
                                player.play();
                                update(&|s| {
                                    s.state = PlaybackState::Playing;
                                    s.duration_ms = duration_ms;
                                });
                                // 下完后改名成正式缓存，并触发一次淘汰
                                cache.spawn_finalize();
                                let prune_root = cache_dir.clone();
                                std::thread::spawn(move || {
                                    std::thread::sleep(Duration::from_secs(2));
                                    ncm_audio::prune_cache(&prune_root);
                                });
                                continue;
                            }
                            Ok(Err(err)) => {
                                eprintln!("渐进缓存解码失败，回退流式: {err}");
                            }
                            Err(err) => {
                                eprintln!("渐进缓存读取端失败，回退流式: {err}");
                            }
                        }
                    }
                    Err(err) => {
                        eprintln!("渐进缓存不可用，回退流式: {err}");
                    }
                }
                let opened = runtime.block_on(ncm_audio::stream::open(&url, &cache_dir));
                match opened.map(Decoder::new) {
                    Ok(Ok(decoder)) => {
                        duration_ms = decoder
                            .total_duration()
                            .map(|d| d.as_millis() as u64)
                            .unwrap_or(0);
                        player.stop();
                        player.append(TrackTap::new(decoder, track_id, events.clone()));
                        player.play();
                        update(&|s| {
                            s.state = PlaybackState::Playing;
                            s.duration_ms = duration_ms;
                        });
                    }
                    Ok(Err(err)) => update(&|s| {
                        s.state = PlaybackState::Error;
                        s.error = Some(err.to_string());
                    }),
                    Err(err) => update(&|s| {
                        s.state = PlaybackState::Error;
                        s.error = Some(err.to_string());
                    }),
                }
            }
            Ok(Command::Enqueue { url, track_id }) => {
                // 预挂载：只 append，不 stop，所以当前这首不会被打断。
                let opened = runtime.block_on(ncm_audio::stream::open(&url, &cache_dir));
                match opened.map(Decoder::new) {
                    Ok(Ok(decoder)) => {
                        player.append(TrackTap::new(decoder, track_id, events.clone()));
                        player.play();
                    }
                    Ok(Err(err)) => {
                        eprintln!("预挂载失败（track {track_id}）: {err}");
                    }
                    Err(err) => {
                        eprintln!("预挂载失败（track {track_id}）: {err}");
                    }
                }
            }
            Ok(Command::Pause) => {
                player.pause();
                update(&|s| s.state = PlaybackState::Paused);
            }
            Ok(Command::Resume) => {
                player.play();
                update(&|s| s.state = PlaybackState::Playing);
            }
            Ok(Command::Stop) => {
                player.stop();
                update(&|s| {
                    s.state = PlaybackState::Idle;
                    s.position_ms = 0;
                    s.duration_ms = 0;
                    s.track_id = None;
                    s.trial_secs = None;
                });
                duration_ms = 0;
            }
            Ok(Command::Seek(position_ms)) => {
                match player.try_seek(Duration::from_millis(position_ms)) {
                    Ok(()) => update(&|s| s.position_ms = position_ms),
                    Err(error) => {
                        // symphonia 对网络流只支持前进 seek（反向报
                        // RandomAccessNotSupported）。这里重开流并丢弃前 position 的采样，
                        // 用几秒的解码换"能拖回去"的体验。
                        eprintln!("[seek] 直接 seek 失败 pos={position_ms}ms: {error:?} current={}", current.is_some());
                        if let Some((url, track_id)) = current.clone() {
                            let was_paused = player.is_paused();
                            // 缓存已经下完就直接读本地文件：解码器可随机访问
                            let cached = cache_file
                                .as_ref()
                                .filter(|path| path.exists())
                                .and_then(|path| match ncm_audio::open_cached(path) {
                                    Ok(decoder) => Some(decoder),
                                    Err(err) => {
                                        eprintln!("[seek] 打开缓存失败: {err}");
                                        None
                                    }
                                });

                            if let Some(decoder) = cached {
                                if let Some(path) = cache_file.as_ref() {
                                    ncm_audio::touch_cache(path);
                                }
                                let fresh = RodioPlayer::connect_new(sink.mixer());
                                if let Ok(volume) = snapshot.lock().map(|s| s.volume) {
                                    fresh.set_volume(volume);
                                }
                                let duration = decoder
                                    .total_duration()
                                    .map(|value| value.as_millis() as u64)
                                    .unwrap_or(duration_ms);
                                fresh.append(TrackTap::new(
                                    decoder.skip_duration(Duration::from_millis(position_ms)),
                                    track_id,
                                    events.clone(),
                                ));
                                fresh.play();
                                if was_paused {
                                    fresh.pause();
                                }
                                player.stop();
                                player = fresh;
                                base_ms = position_ms;
                                update(&|s| {
                                    s.position_ms = position_ms;
                                    s.duration_ms = duration;
                                });
                                eprintln!("[seek] 用本地缓存跳转完成 pos={position_ms}ms base={base_ms}");
                            } else {
                                // 还没下完：重开流 + 丢弃前面的采样
                                player.stop();
                                update(&|s| s.state = PlaybackState::Loading);
                                let opened =
                                    runtime.block_on(ncm_audio::stream::open(&url, &cache_dir));
                                match opened.map(Decoder::new) {
                                    Ok(Ok(decoder)) => {
                                        let fresh = RodioPlayer::connect_new(sink.mixer());
                                        if let Ok(volume) = snapshot.lock().map(|s| s.volume) {
                                            fresh.set_volume(volume);
                                        }
                                        fresh.append(TrackTap::new(
                                            decoder
                                                .skip_duration(Duration::from_millis(position_ms)),
                                            track_id,
                                            events.clone(),
                                        ));
                                        fresh.play();
                                        if was_paused {
                                            fresh.pause();
                                        }
                                        player = fresh;
                                        base_ms = position_ms;
                                        update(&|s| {
                                            s.state = PlaybackState::Playing;
                                            s.position_ms = position_ms;
                                        });
                                    }
                                    _ => eprintln!("[seek] 重开流失败，保持原位置"),
                                }
                            }
                        }
                    }
                }
            }
            Ok(Command::SetVolume(volume)) => {
                let volume = volume.clamp(0.0, 1.0);
                player.set_volume(volume);
                update(&|s| s.volume = volume);
            }
            Err(RecvTimeoutError::Timeout) => {
                let playing = !player.empty();
                let paused = player.is_paused();
                let position = base_ms + player.get_pos().as_millis() as u64;

                update(&|s| {
                    if s.state == PlaybackState::Error {
                        return;
                    }
                    if playing {
                        s.state = if paused {
                            PlaybackState::Paused
                        } else {
                            PlaybackState::Playing
                        };
                        s.position_ms = position;
                        if s.duration_ms == 0 {
                            s.duration_ms = duration_ms;
                        }
                    } else if matches!(s.state, PlaybackState::Playing | PlaybackState::Paused) {
                        // 队列真的空了（TrackEnded 已由 TrackTap 上报）
                        s.state = PlaybackState::Idle;
                        s.position_ms = 0;
                    }
                });
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}


