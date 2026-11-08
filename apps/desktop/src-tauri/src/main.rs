//! Tauri 外壳：窗口、IPC 桥接、状态装配。
//!
//! 这一层刻意保持得很薄——协议在 `ncm-api`，播放器 actor 在 `ncm-core`，
//! 歌词解析在 `ncm-lyrics`；这里只做「命令 → Rust 调用」和「结果 → DTO」。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ncm_api::{endpoints, Client, Playlist, Song, UserProfile};
use ncm_core::{Command, Event, PlaybackState, Player};
use ncm_platform::{MediaCommand, MediaHandle, MediaRequest, MediaTrack};
use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, State, WebviewUrl, WebviewWindowBuilder};

struct CachedUrl {
    url: String,
    expires_at: Instant,
    trial_secs: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

impl RepeatMode {
    fn as_str(self) -> &'static str {
        match self {
            RepeatMode::Off => "off",
            RepeatMode::All => "all",
            RepeatMode::One => "one",
        }
    }

    fn parse(input: &str) -> Self {
        match input {
            "all" => RepeatMode::All,
            "one" => RepeatMode::One,
            _ => RepeatMode::Off,
        }
    }
}

#[derive(Default)]
struct QueueState {
    tracks: Vec<TrackLite>,
    /// 实际播放顺序（shuffle 时是打乱的），元素是 `tracks` 的下标。
    order: Vec<usize>,
    /// 当前在 `order` 里的位置。
    position: usize,
    repeat: RepeatMode,
    shuffle: bool,
    /// 已经预挂载、还没开始播的下一首：(order 位置, track_id)。
    pending: Option<(usize, u64)>,
    /// 漫游模式：播到队尾时自动再取一批（私人 FM 是流，不是固定歌单）。
    radio: bool,
}

impl QueueState {
    fn track_at(&self, position: usize) -> Option<&TrackLite> {
        self.order
            .get(position)
            .and_then(|index| self.tracks.get(*index))
    }

    fn current_id(&self) -> Option<u64> {
        self.track_at(self.position).map(|track| track.id)
    }

    /// 下一个位置；repeat=All 时从末尾回到开头。
    fn next_position(&self) -> Option<usize> {
        if self.order.is_empty() {
            return None;
        }
        if self.position + 1 < self.order.len() {
            Some(self.position + 1)
        } else if self.repeat == RepeatMode::All {
            Some(0)
        } else {
            None
        }
    }

    /// 漫游模式：忽略 repeat，走到队尾就返回 None（交给 refill 续歌）。
    fn next_position_radio(&self) -> Option<usize> {
        if self.order.is_empty() {
            return None;
        }
        if self.position + 1 < self.order.len() {
            Some(self.position + 1)
        } else {
            None
        }
    }

    fn previous_position(&self) -> Option<usize> {
        if self.order.is_empty() {
            return None;
        }
        if self.position > 0 {
            Some(self.position - 1)
        } else if self.repeat == RepeatMode::All {
            Some(self.order.len() - 1)
        } else {
            None
        }
    }
}

struct AppState {
    client: Client,
    player: Player,
    queue: Mutex<QueueState>,
    url_cache: Mutex<HashMap<u64, CachedUrl>>,
    profile: Mutex<Option<UserProfile>>,
    /// 用户偏好（音质、关窗行为）
    settings: Mutex<Settings>,
    /// 音量的权威值：播放器线程的快照有延迟，落盘不能依赖它。
    volume: Mutex<f32>,
    /// 上一次把状态写盘的时间，给音量拖动做节流。
    last_persist: Mutex<Instant>,
}

/// 需要跨会话记住的界面状态：音量、循环、随机，以及整条队列和当前位置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedState {
    volume: f32,
    repeat: String,
    shuffle: bool,
    tracks: Vec<TrackLite>,
    order: Vec<usize>,
    position: usize,
}

impl Default for PersistedState {
    fn default() -> Self {
        Self {
            volume: 0.8,
            repeat: "off".into(),
            shuffle: false,
            tracks: Vec::new(),
            order: Vec::new(),
            position: 0,
        }
    }
}

/// 用户偏好（落盘到 `<data>/settings.json`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Settings {
    /// standard / higher / exhigh / lossless
    level: String,
    /// 关闭窗口时最小化到托盘而不是退出
    close_to_tray: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            level: "exhigh".into(),
            close_to_tray: false,
        }
    }
}

fn normalize_level(level: &str) -> String {
    match level {
        "standard" | "higher" | "exhigh" | "lossless" => level.to_string(),
        _ => "exhigh".to_string(),
    }
}

fn settings_path() -> PathBuf {
    data_root().join("settings.json")
}

fn load_settings() -> Settings {
    let Ok(text) = std::fs::read_to_string(settings_path()) else {
        return Settings::default();
    };
    let mut settings: Settings = serde_json::from_str(&text).unwrap_or_default();
    settings.level = normalize_level(&settings.level);
    settings
}

fn store_settings(settings: &Settings) -> std::io::Result<()> {
    let path = settings_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let text = serde_json::to_string_pretty(settings).unwrap_or_else(|_| "{}".into());
    std::fs::write(path, text)
}

fn state_path() -> PathBuf {
    data_root().join("state.json")
}

/// 把当前状态写盘。失败只记日志，不影响播放。
fn persist_state(state: &AppState) {
    let volume = state.volume.lock().map(|value| *value).unwrap_or(0.8);
    let snapshot = {
        let Ok(queue) = state.queue.lock() else {
            return;
        };
        PersistedState {
            volume,
            repeat: queue.repeat.as_str().to_string(),
            shuffle: queue.shuffle,
            tracks: queue.tracks.clone(),
            order: queue.order.clone(),
            position: queue.position,
        }
    };

    if let Some(parent) = state_path().parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string(&snapshot) {
        let _ = std::fs::write(state_path(), text);
        if let Ok(mut last) = state.last_persist.lock() {
            *last = Instant::now();
        }
    }
}

/// 启动时读回状态。只恢复「有什么」，不自动开始播放。
fn restore_state(state: &AppState) {
    let Ok(text) = std::fs::read_to_string(state_path()) else {
        return;
    };
    let Ok(saved) = serde_json::from_str::<PersistedState>(&text) else {
        return;
    };

    let volume = saved.volume.clamp(0.0, 1.0);
    if let Ok(mut current) = state.volume.lock() {
        *current = volume;
    }
    state.player.send(Command::SetVolume(volume));

    let Ok(mut queue) = state.queue.lock() else {
        return;
    };
    queue.repeat = RepeatMode::parse(&saved.repeat);
    queue.shuffle = saved.shuffle;
    queue.pending = None;

    let valid = !saved.tracks.is_empty()
        && saved.order.len() == saved.tracks.len()
        && saved.order.iter().all(|index| *index < saved.tracks.len());
    if valid {
        queue.position = saved.position.min(saved.order.len().saturating_sub(1));
        queue.tracks = saved.tracks;
        queue.order = saved.order;
    }
}

fn login_path() -> PathBuf {
    data_root().join("login.json")
}

/// 凭据文件只给当前用户读写：去掉继承的 ACE，再单独授权。
fn harden_credentials(path: &Path) {
    #[cfg(windows)]
    {
        use std::process::{Command, Stdio};
        let Ok(user) = std::env::var("USERNAME") else {
            return;
        };
        let _ = Command::new("icacls")
            .arg(path)
            .arg("/inheritance:r")
            .arg("/grant:r")
            .arg(format!("{user}:F"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UserProfileDto {
    user_id: u64,
    nickname: String,
    avatar_url: Option<String>,
}

impl From<UserProfile> for UserProfileDto {
    fn from(profile: UserProfile) -> Self {
        Self {
            user_id: profile.user_id,
            nickname: profile.nickname,
            avatar_url: profile.avatar_url,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PlaylistDto {
    id: u64,
    name: String,
    cover_url: Option<String>,
    track_count: u64,
    special_type: Option<i64>,
}

impl From<Playlist> for PlaylistDto {
    fn from(playlist: Playlist) -> Self {
        Self {
            id: playlist.id,
            name: playlist.name,
            cover_url: playlist.cover_url,
            track_count: playlist.track_count,
            special_type: playlist.special_type,
        }
    }
}

/// 与前端的 `Track` 对齐；`camelCase` 让 TS 侧不用做字段映射。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrackLite {
    id: u64,
    name: String,
    artists: Vec<String>,
    album: String,
    cover_url: Option<String>,
    duration_ms: u64,
    fee: i64,
}

impl From<Song> for TrackLite {
    fn from(song: Song) -> Self {
        Self {
            id: song.id,
            name: song.name,
            artists: song.artists,
            album: song.album,
            cover_url: song.cover_url,
            duration_ms: song.duration_ms,
            fee: song.fee,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PlayerSnapshotDto {
    state: String,
    position_ms: u64,
    duration_ms: u64,
    volume: f32,
    error: Option<String>,
    track_id: Option<u64>,
    trial_secs: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QueueDto {
    tracks: Vec<TrackLite>,
    current_id: Option<u64>,
    repeat: String,
    shuffle: bool,
}

/// 待播清单：按**实际播放顺序**排好的队列，`position` 是当前曲目在其中的下标。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QueueItemsDto {
    tracks: Vec<TrackLite>,
    position: usize,
    /// 是否处于漫游（私人 FM）模式
    radio: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LyricLineDto {
    start_ms: u64,
    end_ms: u64,
    text: String,
    translation: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LyricDto {
    has_yrc: bool,
    lines: Vec<LyricLineDto>,
}

// ---------------------------------------------------------------------------
// 解析与播放
// ---------------------------------------------------------------------------

async fn resolve_url(state: &AppState, id: u64) -> Result<String, String> {
    resolve_full(state, id).await.map(|(url, _)| url)
}

/// 返回 (直链, 试听秒数)。试听片段只有 Play 需要关心。
async fn resolve_full(state: &AppState, id: u64) -> Result<(String, Option<u64>), String> {
    if let Ok(cache) = state.url_cache.lock() {
        if let Some(hit) = cache.get(&id) {
            if hit.expires_at > Instant::now() {
                return Ok((hit.url.clone(), hit.trial_secs));
            }
        }
    }

    // 用户选的音质优先；拿不到就回落到标准音质
    let level = state
        .settings
        .lock()
        .map(|settings| settings.level.clone())
        .unwrap_or_else(|_| "exhigh".to_string());
    let mut chain = vec![level.as_str()];
    if level != "standard" {
        chain.push("standard");
    }
    let resolved = endpoints::play_url_with_fallback(&state.client, id, &chain)
        .await
        .map_err(|err| err.to_string())?;

    // 服务端给 expi（当前 1200s）；留 60s 余量，避免边界上塞给播放器一个已失效地址
    let ttl = Duration::from_secs(resolved.expi_secs.saturating_sub(60).max(30));
    if let Ok(mut cache) = state.url_cache.lock() {
        cache.insert(
            id,
            CachedUrl {
                url: resolved.url.clone(),
                expires_at: Instant::now() + ttl,
                trial_secs: resolved.trial_secs,
            },
        );
    }

    Ok((resolved.url, resolved.trial_secs))
}

/// 播放队列里第 `position` 首，并预挂载下一首（重复单曲除外）。
async fn play_at(state: &AppState, position: usize) -> Result<(), String> {
    let (track, followed_by) = {
        let mut queue = state.queue.lock().map_err(|_| "队列锁失败")?;
        if position >= queue.order.len() {
            return Err("队列里没有这一首".into());
        }
        queue.position = position;

        let track = queue.track_at(position).cloned();
        let followed_by = if queue.repeat == RepeatMode::One {
            None
        } else {
            queue
                .next_position()
                .and_then(|next| queue.track_at(next).cloned().map(|t| (next, t)))
        };
        (track, followed_by)
    };

    let Some(track) = track else {
        return Err("队列为空".into());
    };
    // 位置已经确定，先记一笔，避免播到一半退出丢进度
    persist_state(state);

    let url = resolve_url(state, track.id).await?;
    let trial_secs = state
        .url_cache
        .lock()
        .ok()
        .and_then(|cache| cache.get(&track.id).and_then(|hit| hit.trial_secs));
    state.player.send(Command::Play {
        url,
        track_id: track.id,
        trial_secs,
    });

    if let Some((next_position, next)) = followed_by {
        if let Ok(url) = resolve_url(state, next.id).await {
            state.player.send(Command::Enqueue {
                url,
                track_id: next.id,
            });
            if let Ok(mut queue) = state.queue.lock() {
                queue.pending = Some((next_position, next.id));
            }
        }
    } else if let Ok(mut queue) = state.queue.lock() {
        queue.pending = None;
    }

    Ok(())
}

#[derive(Debug)]
enum EndedAction {
    /// 下一首已经预挂载好了：推进游标，再把后面那首排进去
    Advance(Option<(usize, TrackLite)>),
    /// 单曲循环：重开当前这首
    Replay(usize),
    Stop,
}

async fn on_track_ended(state: &AppState, ended_id: u64) {
    let action = {
        let Ok(mut queue) = state.queue.lock() else {
            return;
        };
        // 手动切歌/重播时可能收到过期事件，忽略
        if queue.current_id() != Some(ended_id) {
            return;
        }

        if let Some((next_position, _)) = queue.pending.take() {
            queue.position = next_position;
            let follow = if queue.repeat == RepeatMode::One {
                None
            } else {
                queue
                    .next_position()
                    .and_then(|pos| queue.track_at(pos).cloned().map(|t| (pos, t)))
            };
            EndedAction::Advance(follow)
        } else if queue.repeat == RepeatMode::One {
            EndedAction::Replay(queue.position)
        } else {
            EndedAction::Stop
        }
    };

    match action {
        EndedAction::Advance(Some((position, track))) => {
            if let Ok(url) = resolve_url(state, track.id).await {
                state.player.send(Command::Enqueue {
                    url,
                    track_id: track.id,
                });
                if let Ok(mut queue) = state.queue.lock() {
                    queue.pending = Some((position, track.id));
                }
            }
        }
        EndedAction::Advance(None) => {}
        EndedAction::Replay(position) => {
            let _ = play_at(state, position).await;
        }
        EndedAction::Stop => state.player.send(Command::Stop),
    }
}

fn shuffle_order(length: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..length).collect();
    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15);
    for i in (1..length).rev() {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let j = (seed % (i as u64 + 1)) as usize;
        order.swap(i, j);
    }
    order
}

// ---------------------------------------------------------------------------
// 命令
// ---------------------------------------------------------------------------

#[tauri::command]
async fn search_tracks(
    state: State<'_, AppState>,
    keyword: String,
    limit: Option<u32>,
) -> Result<Vec<TrackLite>, String> {
    let keyword = keyword.trim();
    if keyword.is_empty() {
        return Ok(Vec::new());
    }

    let limit = limit.unwrap_or(20).clamp(1, 50);
    endpoints::search(&state.client, keyword, limit)
        .await
        .map(|songs| songs.into_iter().map(TrackLite::from).collect())
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn play_queue(
    state: State<'_, AppState>,
    tracks: Vec<TrackLite>,
    start: usize,
) -> Result<(), String> {
    let position = {
        let mut queue = state.queue.lock().map_err(|_| "队列锁失败")?;
        queue.tracks = tracks;
        queue.pending = None;
        queue.radio = false;
        queue.order = if queue.shuffle {
            let mut order = shuffle_order(queue.tracks.len());
            // 让用户点的那首排最前
            if let Some(index) = order.iter().position(|value| *value == start) {
                order.swap(0, index);
            }
            order
        } else {
            (0..queue.tracks.len()).collect()
        };
        queue.position = if queue.shuffle {
            0
        } else {
            start.min(queue.order.len().saturating_sub(1))
        };
        queue.position
    };

    play_at(&state, position).await
}

#[tauri::command]
async fn play_track(state: State<'_, AppState>, id: u64) -> Result<(), String> {
    {
        let mut queue = state.queue.lock().map_err(|_| "队列锁失败")?;
        queue.tracks = vec![];
        queue.order = vec![];
        queue.position = 0;
        queue.pending = None;
        queue.radio = false;
    }
    let url = resolve_url(&state, id).await?;
    state.player.send(Command::Play {
        url,
        track_id: id,
        trial_secs: None,
    });
    persist_state(&state);
    Ok(())
}

#[tauri::command]
async fn next_track(state: State<'_, AppState>) -> Result<(), String> {
    advance(&state).await
}

#[tauri::command]
async fn previous_track(state: State<'_, AppState>) -> Result<(), String> {
    rewind(&state).await
}

async fn advance(state: &AppState) -> Result<(), String> {
    let (position, radio) = {
        let mut queue = state.queue.lock().map_err(|_| "队列锁失败")?;
        queue.pending = None;
        let radio = queue.radio;
        let position = if radio {
            queue.next_position_radio()
        } else {
            queue.next_position()
        };
        (position, radio)
    };
    match position {
        Some(position) => play_at(state, position).await,
        None => {
            // 漫游模式到队尾：再取一批接着放（私人 FM 是流，没有"最后一首"）
            if !radio {
                return Ok(());
            }
            match refill_radio(state).await? {
                Some(next) => play_at(state, next).await,
                None => Ok(()),
            }
        }
    }
}

/// 给漫游队列补一批歌，返回新一批在 order 里的第一个位置。
async fn refill_radio(state: &AppState) -> Result<Option<usize>, String> {
    let mut fresh: Vec<TrackLite> = Vec::new();
    for _ in 0..3 {
        if let Ok(songs) = endpoints::radio_get(&state.client).await {
            fresh.extend(songs.into_iter().map(TrackLite::from));
        }
    }
    if fresh.is_empty() {
        return Ok(None);
    }

    let mut queue = state.queue.lock().map_err(|_| "队列锁失败")?;
    let start = queue.tracks.len();
    let mut seen: std::collections::HashSet<u64> =
        queue.tracks.iter().map(|track| track.id).collect();
    for track in fresh {
        if !seen.insert(track.id) {
            continue;
        }
        let index = queue.tracks.len();
        queue.order.push(index);
        queue.tracks.push(track);
    }
    if queue.tracks.len() == start {
        return Ok(None);
    }
    Ok(Some(start))
}

async fn rewind(state: &AppState) -> Result<(), String> {
    let position = {
        let mut queue = state.queue.lock().map_err(|_| "队列锁失败")?;
        queue.pending = None;
        queue.previous_position()
    };
    match position {
        Some(position) => play_at(state, position).await,
        None => Ok(()),
    }
}

/// 系统媒体面板发来的操作（运行在 tokio 任务里）。
async fn apply_media_command(state: &AppState, command: MediaCommand) {
    match command {
        MediaCommand::Play => state.player.send(Command::Resume),
        MediaCommand::Pause => state.player.send(Command::Pause),
        MediaCommand::Toggle => {
            let playing = state.player.snapshot().state == PlaybackState::Playing;
            state.player.send(if playing {
                Command::Pause
            } else {
                Command::Resume
            });
        }
        MediaCommand::Next => {
            let _ = advance(state).await;
        }
        MediaCommand::Previous => {
            let _ = rewind(state).await;
        }
        MediaCommand::SeekTo(position_ms) => state.player.send(Command::Seek(position_ms)),
        MediaCommand::SeekBy(delta) => {
            let current = state.player.snapshot().position_ms as i64;
            let target = (current + delta).max(0) as u64;
            state.player.send(Command::Seek(target));
        }
    }
}

fn run_media_command(handle: tauri::AppHandle, command: MediaCommand) {
    tauri::async_runtime::spawn(async move {
        let state = handle.state::<AppState>();
        apply_media_command(&state, command).await;
    });
}

fn current_media_track(state: &AppState) -> Option<MediaTrack> {
    let queue = state.queue.lock().ok()?;
    let id = queue.current_id()?;
    let track = queue.tracks.iter().find(|track| track.id == id)?;
    Some(MediaTrack {
        title: track.name.clone(),
        artist: track.artists.join(" / "),
        album: track.album.clone(),
    })
}

// ---------------------------------------------------------------------------
// 桌面歌词窗
// ---------------------------------------------------------------------------

static LYRICS_CLICK_THROUGH: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(true);

fn ensure_lyrics_window(app: &tauri::AppHandle) -> Result<tauri::WebviewWindow, String> {
    if let Some(window) = app.get_webview_window("lyrics") {
        return Ok(window);
    }

    let window =
        WebviewWindowBuilder::new(app, "lyrics", WebviewUrl::App("index.html#lyrics".into()))
            .title("Manbamusic Lyrics")
            .inner_size(900.0, 220.0)
            .decorations(false)
            .transparent(true)
            // WebView 层默认可能是不透明白底，显式给全透明色
            .background_color(tauri::utils::config::Color(0, 0, 0, 0))
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .shadow(false)
            .data_directory(webview_data_dir())
            .build()
            .map_err(|err| err.to_string())?;

    // 贴屏幕底部居中
    if let Ok(Some(monitor)) = window.current_monitor() {
        let size = monitor.size();
        let scale = monitor.scale_factor();
        let width = 900.0_f64;
        let x = (size.width as f64 / scale - width) / 2.0;
        let y = size.height as f64 / scale - 220.0 - 80.0;
        let _ = window.set_position(tauri::LogicalPosition::new(x, y));
    }

    let _ = window
        .set_ignore_cursor_events(LYRICS_CLICK_THROUGH.load(std::sync::atomic::Ordering::Relaxed));
    Ok(window)
}

/// 建/显隐桌面歌词窗。
///
/// 必须是 async：同步命令跑在主线程上，而建窗口和 `current_monitor()`
/// 都需要主线程处理，同步调用会自己等自己。
#[tauri::command]
async fn toggle_lyrics(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let window = ensure_lyrics_window(&app)?;
    if enabled {
        window.show().map_err(|err| err.to_string())?;
    } else {
        window.hide().map_err(|err| err.to_string())?;
    }
    Ok(())
}

/// 点击穿透开关：默认穿透（鼠标能点到下面的应用），关掉后可以拖动窗口。
#[tauri::command]
async fn set_lyrics_click_through(app: tauri::AppHandle, ignore: bool) -> Result<(), String> {
    LYRICS_CLICK_THROUGH.store(ignore, std::sync::atomic::Ordering::Relaxed);
    let window = ensure_lyrics_window(&app)?;
    window
        .set_ignore_cursor_events(ignore)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn set_repeat(state: State<'_, AppState>, mode: String) -> QueueDto {
    if let Ok(mut queue) = state.queue.lock() {
        queue.repeat = RepeatMode::parse(&mode);
    }
    persist_state(&state);
    queue_state(state)
}

#[tauri::command]
async fn toggle_shuffle(state: State<'_, AppState>, enabled: bool) -> Result<QueueDto, String> {
    let (current, position) = {
        let mut queue = state.queue.lock().map_err(|_| "队列锁失败")?;
        let current_track = queue.current_id();
        queue.shuffle = enabled;
        queue.pending = None;

        if enabled {
            let mut order = shuffle_order(queue.tracks.len());
            if let Some(track_id) = current_track {
                if let Some(index) = order
                    .iter()
                    .position(|value| queue.tracks.get(*value).map(|t| t.id) == Some(track_id))
                {
                    order.swap(0, index);
                }
            }
            queue.order = order;
            queue.position = 0;
        } else {
            queue.order = (0..queue.tracks.len()).collect();
            queue.position = current_track
                .and_then(|track_id| queue.tracks.iter().position(|t| t.id == track_id))
                .unwrap_or(0);
        }

        (queue.current_id(), queue.position)
    };

    // 顺序变了，重新给当前这首挂下一首
    if current.is_some() {
        let _ = play_at(&state, position).await;
    }
    persist_state(&state);
    Ok(queue_state(state))
}

#[tauri::command]
fn queue_state(state: State<'_, AppState>) -> QueueDto {
    let Ok(queue) = state.queue.lock() else {
        return QueueDto {
            tracks: Vec::new(),
            current_id: None,
            repeat: "off".into(),
            shuffle: false,
        };
    };

    QueueDto {
        tracks: queue.tracks.clone(),
        current_id: queue.current_id(),
        repeat: queue.repeat.as_str().to_string(),
        shuffle: queue.shuffle,
    }
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state
        .settings
        .lock()
        .map(|settings| settings.clone())
        .unwrap_or_default()
}

#[tauri::command]
fn save_settings(state: State<'_, AppState>, settings: Settings) -> Result<Settings, String> {
    let clean = Settings {
        level: normalize_level(&settings.level),
        close_to_tray: settings.close_to_tray,
    };
    if let Ok(mut current) = state.settings.lock() {
        *current = clean.clone();
    }
    store_settings(&clean).map_err(|error| error.to_string())?;
    Ok(clean)
}

/// 开始漫游（私人 FM）：先取几批凑一个起始队列，之后播到队尾自动续。
#[tauri::command]
async fn play_radio(state: State<'_, AppState>) -> Result<(), String> {
    let mut fresh: Vec<TrackLite> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for _ in 0..3 {
        if let Ok(songs) = endpoints::radio_get(&state.client).await {
            for song in songs {
                let track = TrackLite::from(song);
                if seen.insert(track.id) {
                    fresh.push(track);
                }
            }
        }
    }
    if fresh.is_empty() {
        return Err("漫游暂时没有内容，稍后再试".into());
    }

    {
        let mut queue = state.queue.lock().map_err(|_| "队列锁失败")?;
        queue.tracks = fresh;
        queue.order = (0..queue.tracks.len()).collect();
        queue.position = 0;
        queue.pending = None;
        queue.radio = true;
    }
    play_at(&state, 0).await
}

/// 私人雷达：每天更新的一个歌单，直接从推荐资源里找 id 再取曲目。
#[tauri::command]
async fn radar_tracks(state: State<'_, AppState>) -> Result<Vec<TrackLite>, String> {
    let id = endpoints::private_radar_id(&state.client).await;
    endpoints::playlist_tracks(&state.client, id)
        .await
        .map(|songs| songs.into_iter().map(TrackLite::from).collect())
        .map_err(|error| error.to_string())
}

/// 每日推荐（未登录时网易不给数据，前端会提示先登录）。
#[tauri::command]
async fn daily_songs(state: State<'_, AppState>) -> Result<Vec<TrackLite>, String> {
    endpoints::daily_songs(&state.client)
        .await
        .map(|songs| songs.into_iter().map(TrackLite::from).collect())
        .map_err(|error| error.to_string())
}
/// 待播清单：按播放顺序展开（随机模式下就是打乱后的顺序）。
#[tauri::command]
fn queue_items(state: State<'_, AppState>) -> QueueItemsDto {
    let Ok(queue) = state.queue.lock() else {
        return QueueItemsDto {
            tracks: Vec::new(),
            position: 0,
            radio: false,
        };
    };

    QueueItemsDto {
        tracks: queue
            .order
            .iter()
            .filter_map(|index| queue.tracks.get(*index).cloned())
            .collect(),
        position: queue.position,
        radio: queue.radio,
    }
}

/// 从待播清单的某一项开始播放。
#[tauri::command]
async fn jump_to(state: State<'_, AppState>, index: usize) -> Result<(), String> {
    play_at(&state, index).await
}

#[tauri::command]
fn player_snapshot(state: State<'_, AppState>) -> PlayerSnapshotDto {
    let snapshot = state.player.snapshot();
    PlayerSnapshotDto {
        state: snapshot.state.as_str().to_string(),
        position_ms: snapshot.position_ms,
        duration_ms: snapshot.duration_ms,
        volume: snapshot.volume,
        error: snapshot.error,
        track_id: snapshot.track_id,
        trial_secs: snapshot.trial_secs,
    }
}

#[tauri::command]
fn pause_player(state: State<'_, AppState>) {
    state.player.send(Command::Pause);
}

#[tauri::command]
async fn resume_player(state: State<'_, AppState>) -> Result<(), String> {
    // 重启后播放器是 idle 的，此时「播放」应该从恢复的队列当前位置开播，
    // 否则用户要点两次（先点歌再点播放）。
    let snapshot = state.player.snapshot();
    if snapshot.state == PlaybackState::Idle {
        let position = {
            let Ok(queue) = state.queue.lock() else {
                return Ok(());
            };
            if queue.order.is_empty() {
                return Ok(());
            }
            queue.position
        };
        return play_at(&state, position).await;
    }
    state.player.send(Command::Resume);
    Ok(())
}

#[tauri::command]
fn stop_player(state: State<'_, AppState>) {
    state.player.send(Command::Stop);
}

#[tauri::command]
fn seek_player(state: State<'_, AppState>, position_ms: u64) {
    // 留 2 秒尾巴：拖到最末尾会让当前源直接读完，触发 TrackEnded 切下一首
    let duration = state.player.snapshot().duration_ms;
    let clamped = if duration > 2000 {
        position_ms.min(duration - 2000)
    } else {
        position_ms
    };
    state.player.send(Command::Seek(clamped));
}

#[tauri::command]
fn set_volume(state: State<'_, AppState>, volume: f32) {
    state.player.send(Command::SetVolume(volume));
    if let Ok(mut current) = state.volume.lock() {
        *current = volume.clamp(0.0, 1.0);
    }
    // 拖动音量会高频调用，250ms 内只写一次盘
    let should_write = state
        .last_persist
        .lock()
        .map(|last| last.elapsed() >= Duration::from_millis(250))
        .unwrap_or(true);
    if should_write {
        persist_state(&state);
    }
}

/// 喜欢 / 取消喜欢一首歌。返回操作后的状态，前端据此更新红心。
#[tauri::command]
async fn like_song(state: State<'_, AppState>, id: u64, like: bool) -> Result<bool, String> {
    endpoints::like(&state.client, id, like)
        .await
        .map(|_| like)
        .map_err(|error| error.to_string())
}


/// 批量查询已喜欢的曲目；失败时返回空集合（红心只是装饰，不阻塞列表）。
#[tauri::command]
async fn liked_songs(state: State<'_, AppState>, ids: Vec<u64>) -> Result<Vec<u64>, String> {
    endpoints::liked_ids(&state.client, &ids)
        .await
        .map(|liked| liked.into_iter().collect())
        .map_err(|error| error.to_string())
}

// ---------------------------------------------------------------------------
// 登录
// ---------------------------------------------------------------------------

#[tauri::command]
async fn login_qr_start(state: State<'_, AppState>) -> Result<String, String> {
    endpoints::qr_unikey(&state.client)
        .await
        .map_err(|err| err.to_string())
}

/// 轮询扫码结果；803 时把会话落盘并顺手拉一次账号信息。
#[tauri::command]
async fn login_qr_poll(state: State<'_, AppState>, key: String) -> Result<String, String> {
    let status = endpoints::qr_check(&state.client, &key)
        .await
        .map_err(|err| err.to_string())?;

    if status == endpoints::QrStatus::Success {
        let _ = state.client.save_cookies(&login_path());
        harden_credentials(&login_path());
        // 未登录时缓存下来的可能是 45 秒试听地址，登录后必须整体作废
        if let Ok(mut cache) = state.url_cache.lock() {
            cache.clear();
        }
        if let Ok(Some(profile)) = endpoints::account(&state.client).await {
            if let Ok(mut slot) = state.profile.lock() {
                *slot = Some(profile);
            }
        }
    }

    Ok(status.as_str().to_string())
}

#[tauri::command]
async fn my_profile(state: State<'_, AppState>) -> Result<Option<UserProfileDto>, String> {
    let cached = state.profile.lock().ok().and_then(|slot| slot.clone());
    if let Some(profile) = cached {
        return Ok(Some(profile.into()));
    }

    if !state.client.is_logged_in() {
        return Ok(None);
    }

    match endpoints::account(&state.client).await {
        Ok(Some(profile)) => {
            if let Ok(mut slot) = state.profile.lock() {
                *slot = Some(profile.clone());
            }
            Ok(Some(profile.into()))
        }
        Ok(None) => Ok(None),
        Err(err) => Err(err.to_string()),
    }
}

#[tauri::command]
fn logout(state: State<'_, AppState>) -> Result<(), String> {
    state.client.logout();
    if let Ok(mut slot) = state.profile.lock() {
        *slot = None;
    }
    // 退出后直链缓存里的会员地址也不能再用了
    if let Ok(mut cache) = state.url_cache.lock() {
        cache.clear();
    }
    let _ = std::fs::remove_file(login_path());
    Ok(())
}

#[tauri::command]
async fn my_playlists(state: State<'_, AppState>) -> Result<Vec<PlaylistDto>, String> {
    let uid = {
        let cached = state.profile.lock().ok().and_then(|slot| slot.clone());
        match cached {
            Some(profile) => profile.user_id,
            None => {
                let fetched = endpoints::account(&state.client)
                    .await
                    .map_err(|err| err.to_string())?;
                match fetched {
                    Some(profile) => {
                        let id = profile.user_id;
                        if let Ok(mut slot) = state.profile.lock() {
                            *slot = Some(profile);
                        }
                        id
                    }
                    None => return Err("未登录".to_string()),
                }
            }
        }
    };

    endpoints::user_playlists(&state.client, uid)
        .await
        .map(|list| list.into_iter().map(PlaylistDto::from).collect())
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn playlist_tracks(state: State<'_, AppState>, id: u64) -> Result<Vec<TrackLite>, String> {
    endpoints::playlist_tracks(&state.client, id)
        .await
        .map(|songs| songs.into_iter().map(TrackLite::from).collect())
        .map_err(|err| err.to_string())
}

/// 取歌词：优先逐字（yrc），没有就回落 lrc，再合并翻译。
#[tauri::command]
async fn get_lyric(state: State<'_, AppState>, id: u64) -> Result<LyricDto, String> {
    let raw = endpoints::lyric(&state.client, id)
        .await
        .map_err(|err| err.to_string())?;

    let mut track = raw
        .yrc
        .as_deref()
        .map(ncm_lyrics::parse_yrc)
        .unwrap_or_default();
    if track.lines.is_empty() {
        track = raw
            .lrc
            .as_deref()
            .map(ncm_lyrics::parse_lrc)
            .unwrap_or_default();
    }

    if let Some(translation) = raw.tlyric.as_deref() {
        let parsed = ncm_lyrics::parse_lrc(translation);
        ncm_lyrics::merge_translation(&mut track, &parsed);
    }

    Ok(LyricDto {
        has_yrc: track.has_yrc,
        lines: track
            .lines
            .into_iter()
            .map(|line| LyricLineDto {
                start_ms: line.start_ms,
                end_ms: line.end_ms,
                text: line.text,
                translation: line.translation,
            })
            .collect(),
    })
}

/// 冒烟测试专用：把结果回传给主进程并退出。未开启冒烟时是空操作。
#[tauri::command]
fn smoke_report(value: String) {
    if std::env::var_os("MANBAMUSIC_SMOKE").is_none() {
        return;
    }
    println!("{value}");
    std::process::exit(if value.starts_with("SMOKE_OK") { 0 } else { 1 });
}

// ---------------------------------------------------------------------------
// 路径与启动
// ---------------------------------------------------------------------------

/// 启动日志：双击运行失败时（无签名被拦、目录不可写等）这是唯一线索。
/// 优先写到 exe 同目录，不行再退到临时目录；整个过程不 panic。
fn startup_log_path() -> PathBuf {
    static LOG: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    LOG.get_or_init(|| {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let path = dir.join("startup.log");
                if std::fs::write(&path, b"").is_ok() {
                    return path;
                }
            }
        }
        let fallback = std::env::temp_dir().join("Manbamusic-startup.log");
        let _ = std::fs::write(&fallback, b"");
        fallback
    })
    .clone()
}

fn log_startup(message: &str) {
    use std::io::Write;
    let path = startup_log_path();
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(file, "[{}] {message}", unix_stamp());
    }
}

fn unix_stamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}.{:03}", now.as_secs(), now.subsec_millis())
}

/// 回退只在第一次记一条，避免每次 data_root() 都刷日志。
fn log_fallback_once(path: &Path) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| log_startup(&format!("数据目录回退到 {}", path.display())));
}

fn data_root() -> PathBuf {
    if let Some(dir) = std::env::var_os("MANBAMUSIC_DATA_DIR") {
        return PathBuf::from(dir);
    }

    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);

    let preferred = base.join("Manbamusic");
    if writable(&preferred) {
        return preferred;
    }

    // 便携模式：跟 exe 放一起（U 盘、解压到桌面都能用）
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let portable = dir.join("Manbamusic-data");
            if writable(&portable) {
                log_fallback_once(&portable);
                return portable;
            }
        }
    }

    let fallback = std::env::temp_dir().join("Manbamusic");
    if writable(&fallback) {
        log_fallback_once(&fallback);
        return fallback;
    }

    log_startup("所有数据目录都不可写，程序无法继续");
    eprintln!("所有候选数据目录都不可写");
    std::process::exit(2);
}

fn webview_data_dir() -> PathBuf {
    std::env::var_os("MANBAMUSIC_WEBVIEW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| data_root().join("EBWebView"))
}

fn writable(dir: &Path) -> bool {
    if let Err(error) = std::fs::create_dir_all(dir) {
        eprintln!("[data] 创建 {} 失败: {error}", dir.display());
        return false;
    }
    let probe = dir.join(".write-probe");
    match std::fs::write(&probe, b"1") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(error) => {
            eprintln!("[data] 写入 {} 失败: {error}", probe.display());
            false
        }
    }
}

fn main() {
    // 先落一条启动记录：双击没反应时用户需要这个文件
    let root = data_root();
    log_startup(&format!(
        "启动 exe={:?} 数据目录={:?}",
        std::env::current_exe().unwrap_or_default(),
        root
    ));

    let client = match Client::new() {
        Ok(client) => client,
        Err(err) => {
            eprintln!("初始化网易云客户端失败: {err}");
            std::process::exit(1);
        }
    };
    // 上次登录的会话（MUSIC_U 等）；文件不存在就是匿名态
    if client.load_cookies(&login_path()) {
        eprintln!("已恢复登录会话");
        // 旧版本可能留下宽松权限，启动时再收紧一次
        harden_credentials(&login_path());
    }

    let runtime = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("创建下载 runtime 失败"),
    );
    let data_dir = data_root();
    let (player, mut events) = Player::spawn(runtime.clone(), data_dir.join("cache"));

    tauri::Builder::default()
        .manage(AppState {
            client,
            player,
            queue: Mutex::new(QueueState::default()),
            url_cache: Mutex::new(HashMap::new()),
            profile: Mutex::new(None),
            settings: Mutex::new(load_settings()),
            volume: Mutex::new(0.8),
            last_persist: Mutex::new(Instant::now() - Duration::from_secs(10)),
        })
        .setup(move |app| {
            // 恢复上次的音量、循环/随机模式与队列（不自动播放）
            restore_state(app.state::<AppState>().inner());
            let data_dir = webview_data_dir();
            let window =
                WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                    .title("Manbamusic")
                    .inner_size(1180.0, 760.0)
                    .min_inner_size(920.0, 560.0)
                    .resizable(true)
                    // 标题栏交给前端画，去掉 Windows 原生窗口框
                    .decorations(false)
                    .data_directory(data_dir)
                    .build()?;

            // 双保险：部分 Windows 环境下 builder 的 decorations 不生效，
            // 建成后再显式去掉一次原生边框（前端自绘标题栏）。
            match window.set_decorations(false) {
                Ok(()) => eprintln!("[window] decorations cleared"),
                Err(error) => eprintln!("[window] set_decorations failed: {error}"),
            }
            eprintln!("[window] is_decorated now = {:?}", window.is_decorated());

            // 关闭窗口时：可选最小化到托盘而不是退出
            {
                let handle = app.handle().clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        let to_tray = handle
                            .state::<AppState>()
                            .settings
                            .lock()
                            .map(|settings| settings.close_to_tray)
                            .unwrap_or(false);
                        if to_tray {
                            api.prevent_close();
                            if let Some(main) = handle.get_webview_window("main") {
                                let _ = main.hide();
                            }
                        }
                    }
                });
            }

            // ---- 托盘 ----
            let toggle_item = MenuItem::with_id(app, "toggle", "播放 / 暂停", true, None::<&str>)?;
            let previous_item = MenuItem::with_id(app, "previous", "上一首", true, None::<&str>)?;
            let next_item = MenuItem::with_id(app, "next", "下一首", true, None::<&str>)?;
            let show_item = MenuItem::with_id(app, "show", "显示窗口", true, None::<&str>)?;
            let lyrics_item = MenuItem::with_id(app, "lyrics", "桌面歌词", true, None::<&str>)?;
            let lyrics_lock_item =
                MenuItem::with_id(app, "lyrics_lock", "歌词解锁/锁定", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[
                    &toggle_item,
                    &previous_item,
                    &next_item,
                    &show_item,
                    &lyrics_item,
                    &lyrics_lock_item,
                    &quit_item,
                ],
            )?;

            if let Some(icon) = app.default_window_icon().cloned() {
                TrayIconBuilder::new()
                    .icon(icon)
                    .tooltip("Manbamusic")
                    .menu(&menu)
                    .show_menu_on_left_click(false)
                    .on_menu_event(|app, event| {
                        let handle = app.clone();
                        match event.id.as_ref() {
                            "toggle" => run_media_command(handle, MediaCommand::Toggle),
                            "previous" => run_media_command(handle, MediaCommand::Previous),
                            "next" => run_media_command(handle, MediaCommand::Next),
                            "show" => {
                                if let Some(window) = handle.get_webview_window("main") {
                                    let _ = window.show();
                                    let _ = window.set_focus();
                                }
                            }
                            "lyrics" => {
                                if let Ok(window) = ensure_lyrics_window(app) {
                                    let visible = window.is_visible().unwrap_or(false);
                                    if visible {
                                        let _ = window.hide();
                                    } else {
                                        let _ = window.show();
                                    }
                                }
                            }
                            "lyrics_lock" => {
                                let current =
                                    LYRICS_CLICK_THROUGH.load(std::sync::atomic::Ordering::Relaxed);
                                LYRICS_CLICK_THROUGH
                                    .store(!current, std::sync::atomic::Ordering::Relaxed);
                                if let Ok(window) = ensure_lyrics_window(app) {
                                    let _ = window.set_ignore_cursor_events(!current);
                                }
                            }
                            "quit" => handle.exit(0),
                            _ => {}
                        }
                    })
                    .build(app)?;
            }

            // ---- 系统媒体键 / 媒体面板 ----
            let hwnd = window.hwnd().ok().map(|handle| handle.0 as usize);
            let app_handle = app.handle().clone();
            if let Some(media) = MediaHandle::spawn(hwnd, move |command| {
                // 回调跑在系统线程上，只做转发
                let handle = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    let state = handle.state::<AppState>();
                    apply_media_command(&state, command).await;
                });
            }) {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    loop {
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        let state = handle.state::<AppState>();
                        let snapshot = state.player.snapshot();
                        media.sync(MediaRequest {
                            track: current_media_track(&state),
                            playing: snapshot.state == PlaybackState::Playing,
                            position_ms: snapshot.position_ms,
                            duration_ms: snapshot.duration_ms,
                        });
                    }
                });
            }

            // 队列自动前进：一首自然播完 → 推进游标 + 预挂载再下一首。
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while let Some(Event::TrackEnded { track_id }) = events.recv().await {
                    let state = handle.state::<AppState>();
                    on_track_ended(&state, track_id).await;
                }
            });

            // 端到端冒烟：真正从 WebView 里调一次 invoke，验证 IPC + 协议层都通。
            if std::env::var_os("MANBAMUSIC_SMOKE").is_some() {
                let window = window.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_secs(3));
                    let script = "const I=(c,a)=>window.__TAURI_INTERNALS__.invoke(c,a);\
                        new Promise(r=>setTimeout(r,1200))\
                        .then(()=>I('smoke_report',{value:'SMOKE_OK:rows='+document.querySelectorAll('.tracks li').length}))\
                        .catch(e=>I('smoke_report',{value:'SMOKE_ERR:'+e}))";
                    if window.eval(script).is_err() {
                        println!("SMOKE_EVAL_FAILED");
                        std::process::exit(2);
                    }
                    std::thread::sleep(Duration::from_secs(45));
                    println!("SMOKE_TIMEOUT");
                    std::process::exit(3);
                });
            }

            let _ = runtime;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            search_tracks,
            play_queue,
            play_track,
            next_track,
            previous_track,
            queue_state,
            queue_items,
            daily_songs,
            play_radio,
            radar_tracks,
            get_settings,
            save_settings,
            jump_to,
            set_repeat,
            toggle_shuffle,
            player_snapshot,
            pause_player,
            resume_player,
            stop_player,
            seek_player,
            set_volume,
  like_song,
  liked_songs,
            get_lyric,
            login_qr_start,
            login_qr_poll,
            my_profile,
            logout,
            my_playlists,
            playlist_tracks,
            toggle_lyrics,
            set_lyrics_click_through,
            smoke_report
        ])
        .run(tauri::generate_context!())
        .expect("Tauri 启动失败");
}



























