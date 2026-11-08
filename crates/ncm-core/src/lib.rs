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

    let player = RodioPlayer::connect_new(sink.mixer());
    if let Ok(volume) = snapshot.lock().map(|s| s.volume) {
        player.set_volume(volume);
    }

    let mut duration_ms = 0u64;

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
                let _ = player.try_seek(Duration::from_millis(position_ms));
                update(&|s| s.position_ms = position_ms);
            }
            Ok(Command::SetVolume(volume)) => {
                let volume = volume.clamp(0.0, 1.0);
                player.set_volume(volume);
                update(&|s| s.volume = volume);
            }
            Err(RecvTimeoutError::Timeout) => {
                let playing = !player.empty();
                let paused = player.is_paused();
                let position = player.get_pos().as_millis() as u64;

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
