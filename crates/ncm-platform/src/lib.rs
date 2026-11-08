//! 系统媒体集成：把播放器接到操作系统的媒体键 / 媒体面板（Windows SMTC、
//! Linux MPRIS、macOS Now Playing）。
//!
//! `MediaControls` 只能在创建它的线程里用，而且 `attach` 的回调跑在系统线程上，
//! 所以这里用一条专用线程持有它：外部只发 [`MediaRequest`]，命令通过回调转成
//! [`MediaCommand`] 交给上层。

use std::sync::mpsc::{self, Sender};
use std::time::Duration;

use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig,
    SeekDirection,
};

/// 系统媒体面板 / 媒体键发来的操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaCommand {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    SeekTo(u64),
    SeekBy(i64),
}

/// 当前曲目的展示信息。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MediaTrack {
    pub title: String,
    pub artist: String,
    pub album: String,
}

/// 上层推给系统面板的状态。
#[derive(Debug, Clone, Default)]
pub struct MediaRequest {
    pub track: Option<MediaTrack>,
    pub playing: bool,
    pub position_ms: u64,
    pub duration_ms: u64,
}

/// 媒体集成的句柄；后台线程持有真正的 `MediaControls`。
pub struct MediaHandle {
    tx: Sender<MediaRequest>,
}

impl MediaHandle {
    /// 启动媒体集成线程。
    ///
    /// Windows 需要窗口句柄（`hwnd`），所以必须在窗口创建之后调用。
    /// 返回 `None` 表示当前平台/环境拿不到媒体控制（不影响播放）。
    pub fn spawn<F>(hwnd: Option<usize>, on_command: F) -> Option<Self>
    where
        F: Fn(MediaCommand) + Send + 'static,
    {
        let (request_tx, request_rx) = mpsc::channel::<MediaRequest>();
        let (ready_tx, ready_rx) = mpsc::channel::<bool>();

        std::thread::Builder::new()
            .name("ncm-media".into())
            .spawn(move || {
                // 裸指针不是 Send，所以在子线程里才转成指针
                let hwnd = hwnd.map(|value| value as *mut std::ffi::c_void);
                let config = PlatformConfig {
                    dbus_name: "manbamusic",
                    display_name: "Manbamusic",
                    hwnd,
                };

                let mut controls = match MediaControls::new(config) {
                    Ok(controls) => controls,
                    Err(err) => {
                        eprintln!("系统媒体控制不可用: {err}");
                        let _ = ready_tx.send(false);
                        return;
                    }
                };

                if let Err(err) = controls.attach(move |event: MediaControlEvent| {
                    let command = match event {
                        MediaControlEvent::Play => Some(MediaCommand::Play),
                        MediaControlEvent::Pause => Some(MediaCommand::Pause),
                        MediaControlEvent::Toggle => Some(MediaCommand::Toggle),
                        MediaControlEvent::Next => Some(MediaCommand::Next),
                        MediaControlEvent::Previous => Some(MediaCommand::Previous),
                        MediaControlEvent::Seek(direction) => Some(MediaCommand::SeekBy(
                            if direction == SeekDirection::Forward {
                                5_000
                            } else {
                                -5_000
                            },
                        )),
                        MediaControlEvent::SeekBy(direction, delta) => Some(MediaCommand::SeekBy(
                            let_ms(delta)
                                * if direction == SeekDirection::Forward {
                                    1
                                } else {
                                    -1
                                },
                        )),
                        MediaControlEvent::SetPosition(position) => {
                            Some(MediaCommand::SeekTo(position.0.as_millis() as u64))
                        }
                        _ => None,
                    };
                    if let Some(command) = command {
                        on_command(command);
                    }
                }) {
                    eprintln!("attach 媒体回调失败: {err}");
                    let _ = ready_tx.send(false);
                    return;
                }

                let _ = ready_tx.send(true);

                let mut last: Option<MediaRequest> = None;
                while let Ok(request) = request_rx.recv() {
                    let metadata_changed = last
                        .as_ref()
                        .map(|previous| previous.track != request.track)
                        .unwrap_or(true);

                    if metadata_changed {
                        let metadata = MediaMetadata {
                            title: request.track.as_ref().map(|t| t.title.as_str()),
                            artist: request.track.as_ref().map(|t| t.artist.as_str()),
                            album: request.track.as_ref().map(|t| t.album.as_str()),
                            duration: if request.duration_ms > 0 {
                                Some(Duration::from_millis(request.duration_ms))
                            } else {
                                None
                            },
                            ..Default::default()
                        };
                        let _ = controls.set_metadata(metadata);
                    }

                    let playback = if request.playing {
                        MediaPlayback::Playing {
                            progress: Some(MediaPosition(Duration::from_millis(
                                request.position_ms,
                            ))),
                        }
                    } else {
                        MediaPlayback::Paused {
                            progress: Some(MediaPosition(Duration::from_millis(
                                request.position_ms,
                            ))),
                        }
                    };
                    let _ = controls.set_playback(playback);

                    last = Some(request);
                }
            })
            .expect("创建媒体线程失败");

        match ready_rx.recv() {
            Ok(true) => Some(Self { tx: request_tx }),
            _ => None,
        }
    }

    /// 推送最新状态；没变化时上层可以不推。
    pub fn sync(&self, request: MediaRequest) {
        let _ = self.tx.send(request);
    }
}

fn let_ms(duration: Duration) -> i64 {
    duration.as_millis() as i64
}
