# Manbamusic · Rust 第三方网易云音乐播放器 · 架构方案 v2

> 本文记录当前采用的架构：**核对并修正了早期草案里的 9 处事实问题（其中 3 处会直接编译失败）**，补齐了草案缺失的 4 个模块（数据层、平台集成、歌词引擎、IPC 契约），并给出一条"每一步都能点开听歌"的落地路径。
>
> 文中所有"实测"结论都可以用 `tools/ncm-crypto-probe.mjs` 复现（见文末附录 A）。

---

## 0. 一页结论

| 维度 | 结论 | 理由 |
| --- | --- | --- |
| UI | **Tauri v2**（2.12.x）+ 薄前端，Rust 承担 100% 逻辑 | 歌词滚动、毛玻璃、歌单网格这类视觉密度，纯 Rust UI 的返工成本远高于 WebView 的 80MB |
| 音频 | **rodio 0.23** 起步，内部留 `AudioOutput` trait 口子 | 0.22 起 API 重构（Sink→Player / OutputStream→MixerDeviceSink），队列预挂载可做到真无缝 |
| 数据 | **rusqlite**（bundled）+ 专用 DB 线程 | 桌面端单机、万级行数；sqlx 的编译期宏对 CI 不友好，redb 没有查询能力 |
| 协议 | **weapi 为主、eapi 为辅**，全局节流 | 两者都已实测可用，但 eapi 参数集更窄（多传一个字段就 404） |
| 线程 | 3 线程 + Actor：UI / tokio 核心 / 音频线程 | `MixerDeviceSink` 内含 `cpal::Stream`，**不是 Send**，放不进 tokio 任务 |

三句话概括早期草案的问题：

1. **代码是按 rodio 0.17 写的**（`OutputStream::try_default()` / `Sink::try_new()`），照抄在 0.23 上编译不过，而且 `OutputStream` 根本不能放进 `tokio::spawn`。
2. **"随手几百行就能跑"的部分写多了，"上线必踩"的部分写少了**：URL 只在 20 分钟内有效（响应里 `expi: 1200`）、外层 `code: 200` 不代表成功、yrc 逐字歌词并非每首都有。
3. **缺了工程闭环**：没有状态机、没有数据 schema、没有缓存淘汰、没有热插拔、没有打包与签名。

---

## 1. 先修正早期草案的 9 处事实问题

| # | 早期草案的说法 | 事实（2026-11 核对） | 后果 |
| --- | --- | --- | --- |
| 1 | `rodio::OutputStream::try_default()` + `Sink::try_new()` | rodio **0.21** 起 `OutputStream` 改名 `MixerDeviceSink`、`Sink` 改名 `Player`；`DeviceSinkBuilder::open_default_sink()` 建流，`Player::connect_new(mixer)` 建播放器 | 代码直接编译失败 |
| 2 | 把 `OutputStream` 初始化放进 `tokio::spawn` 的任务里 | `MixerDeviceSink` 持有 `cpal::Stream`，**不是 `Send`**；放进 tokio 任务会因 future 不满足 `Send` 而编译失败，强行绕过还会有线程迁移风险 | 卡在类型系统上，或者得到偶发爆音 |
| 3 | "symphonia 开启 all 格式支持" | rodio 现有 feature 叫 `symphonia-all`；symphonia 本体已到 **0.6.1**，feature 拆成 `all-formats` / `all-codecs` | 依赖声明报错 |
| 4 | "20 分钟过期" | 准确，而且响应里直接给了 `expi: 1200`（秒） | 可用来驱动缓存与刷新 |
| 5 | （隐含）拿到 `url` 就成功 | **外层 `code: 200` 恒成立**，真实结果在 `data[0].code`；匿名态请求《晴天》得到 `data[0].code = 404` | 不判断内层码 → 播放器白屏无报错 |
| 6 | "HTTP 流边下边播、随机 Seek" | 实测支持：`Range: bytes=1024-2047` 返回 **206** 且 `Content-Range` 正确；但**响应头没有 `Accept-Ranges`** | 用 header 判断能力会误判为"不支持 seek" |
| 7 | "souvlaki 封装三大平台" | 仍活跃（0.9.3，2026-10-27 更新），但回调在 OS 线程触发、三平台封面传参不同 | 在回调里直接操作 UI 会死锁 |
| 8 | 每 100ms 通过通道推进度给 UI | 该频率若走 Tauri IPC（JSON 序列化 + 跨进程）会造成明显抖动 | 改成 2Hz + 前端本地插值 |
| 9 | 逐字歌词"解析 YRC" | YRC 形如 `[28480,11820](28480,160,0)我(28640,420,0)带…`；**很多歌没有 yrc**（实测《晴天》只有 lrc+tlyric） | 必须实现 yrc→lrc 回落，否则歌词区空白 |

> 另外两个不是"错"、但会拖后腿的判断：`sqlx` 不是桌面端默认解（见 §2.3）；`global-hotkey` 在 Tauri 下直接用 `tauri-plugin-global-shortcut` 更省事。

---

## 2. 选型：结论、取舍与退出条件

### 2.1 GUI：Tauri v2（推荐）vs Iced / GPUI

| 维度 | Tauri v2 | Iced / GPUI |
| --- | --- | --- |
| 首屏、歌词滚动、毛玻璃 | 浏览器引擎免费给你，CSS/GPU 合成糖 | 要自己写布局、着色器、惯性滚动 |
| 内存（播放态） | 后端 ~20MB + WebView2 ~80-150MB | 20-40MB |
| 桌面歌词窗 | 一个独立 `WebviewWindow` + `set_ignore_cursor_events` | 要自己实现透明窗与点击穿透 |
| 生态 | 托盘/单实例/更新/快捷键/窗口状态全是现成插件 | 大量轮子自己造 |
| 换框架成本 | 逻辑全在 `ncm-core`，只有 `ncm-gui` 要重写 | — |

**结论**：选 Tauri v2（当前稳定版 2.12.5；3.0 还在 alpha，不要碰）。前端框架定为 **Svelte 5 + Vite**（用户 2026-11-06 拍板）：打包体积约为 React 的一半，runes 的状态模型和"快照 + 事件"的同步方式很贴合。

**退出条件**（什么情况下这个决策该推翻）：安装包必须 < 5MB，或目标环境（老旧 Windows/Linux 精简版）没有 WebView2/WebKitGTK 且无法内置。出现这两条再评估 Iced——因为 crate 划分已经保证了迁移只影响 `ncm-gui`。

### 2.2 音频：rodio 0.23（推荐）vs 自研 cpal + symphonia

| 需求 | rodio 够不够 |
| --- | --- |
| 播放/暂停/音量/seek | 够（`Player::try_seek`，源需 `Seek`） |
| 切歌无缝 | 够：`Player` 队列预挂载下一首，混音器不断流 |
| 淡入淡出 | 够：`Source::fade_out` / `linear_gain_ramp` |
| 频谱 | 够：插一层采样 tap 到环形缓冲，旁路线程做 FFT |
| EQ | 够：插 biquad 链（`biquad` crate） |
| 交叉淡化 | 勉强：两个 `Player` 同时挂到同一 `Mixer` 手动控音量 |
| WASAPI 独占 / bit-perfect | **不够**，必须自研 |

**结论**：v1 用 rodio。`ncm-audio` 对外只暴露自己的 `AudioOutput` trait（`play/pause/seek/set_volume/load`），内部实现可替换——真到了要做独占输出那天，换实现不动上层。

### 2.3 数据：rusqlite（推荐）vs sqlx vs redb

- **sqlx**：编译期宏需要 `DATABASE_URL` 指向一个已迁移的库，CI/新克隆必炸；且 sqlite 驱动本质仍是同步。
- **redb**：纯 KV，没有 `ORDER BY`、没有模糊搜索，歌单题解、搜索历史都要自己建索引。
- **rusqlite**：`bundled` 特性自带 SQLite 源码，零系统依赖；WAL 模式；配一个专用 DB 线程即可，不阻塞 tokio。

**结论**：rusqlite 0.41 + `bundled`，migrations 用 `PRAGMA user_version` 手写（20 行，比引 framework 稳）。

### 2.4 依赖清单（版本已核对到 2026-11）

```toml
[workspace.dependencies]
tokio       = { version = "1", features = ["rt-multi-thread", "macros", "sync", "time", "fs"] }
reqwest     = { version = "0.12", default-features = false, features = ["json", "cookies", "native-tls", "gzip"] }
rodio       = { version = "0.23", default-features = false, features = ["playback", "symphonia-all"] }
stream-download = "0.24"
rusqlite    = { version = "0.41", features = ["bundled", "chrono"] }
serde       = { version = "1", features = ["derive"] }
serde_json  = "1"
aes         = "0.8"
cbc         = "0.1"
ecb         = "0.1"
rsa         = "0.9"
md-5        = "0.10"
hex         = "0.4"
base64      = "0.22"
rand        = "0.8"
thiserror   = "2"
tracing     = "0.1"
souvlaki    = "0.9"
lofty       = "0.25"        # 本地音乐标签（可选）
rubato      = "5"           # 重采样（可选）
rustfft     = "6"           # 频谱（可选）
```

两条环境前提：**stream-download 0.24 的 MSRV 是 1.95**，所以整套工具链要装最新 stable（1.95+）。

`reqwest` 这里用的是 **native-tls（Windows 上就是 schannel）** 而不是 rustls：rustls 0.23 的默认 provider 是 aws-lc-rs，需要 CMake/NASM，ring 也需要 C 工具链——在本机「GNU 工具链 + 没有 MSVC」的环境里会直接卡住。装了 VS Build Tools 之后可以切回 `rustls-tls`（跨平台更省心）。

---

## 3. Workspace 结构

```
manbamusic/
├── Cargo.toml                  # [workspace]，统一 profile 与依赖版本
├── crates/
│   ├── ncm-api/                # 协议层：加密、HTTP、Cookie、端点。不碰 UI/音频
│   │   └── src/{crypto,client,jar,endpoints/*,models}.rs
│   ├── ncm-store/              # 持久化：SQLite、migrations、凭据、缓存清单
│   │   └── src/{db,migrations,repo/*,credentials}.rs
│   ├── ncm-lyrics/             # 纯函数：LRC/YRC/tlyric 解析、对齐、二分定位
│   │   └── src/{parse,yrc,lrc,model}.rs
│   ├── ncm-audio/              # 音频：设备、Player、预取流、tap/EQ、重采样
│   │   └── src/{output,engine,stream_cache,dsp}.rs
│   ├── ncm-platform/           # 系统集成：媒体键、托盘菜单、通知、防休眠
│   │   └── src/{media,notify,power}.rs
│   ├── ncm-core/               # 业务中枢：状态机、队列、URL 缓存、预取、事件广播
│   │   └── src/{actor,state,queue,resolver,prefetch}.rs
│   └── ncm-ipc/                # 前后端共享 DTO（ts-rs 生成 TS 类型）
└── apps/
    ├── desktop/                # Tauri：main.rs 只做装配 + command 桥接
    └── cli/                    # 可选：无 UI 的播放/搜索，用来压测协议层
```

**依赖方向（只能单向）**：`ipc ← core ← {api, store, lyrics, audio, platform}`。`audio` 不认识 `api`，`api` 不认识 `store`。这条规矩的直接收益：协议层可以用 `apps/cli` 单独跑，音频层可以用假数据单独测。

比早期草案多拆的三个 crate 是**有明确边界的**，不是为拆而拆：

- `ncm-store`：Cookie 落盘要走系统凭据存储，和业务逻辑混在一起会逼你到处传 `&Db`；
- `ncm-lyrics`：这是最值得写单元测试的部分（纯函数、边界多），单独拆出来测试不用起 tokio；
- `ncm-ipc`：Tauri 的 IPC 契约一旦和 Rust 结构体漂移，就是运行时才发现的空字段，用 `ts-rs` 生成类型可以把这类问题挡在编译期。

---

## 4. 运行时架构：三线程 + Actor

### 4.1 线程模型

```
┌────────────────────── WebView (UI 线程) ──────────────────────┐
│  只读快照 + 发命令；进度条/歌词靠本地 rAF 插值                  │
└───────────▲───────────────────────────────┬───────────────────┘
            │ Tauri event (低频)             │ invoke(Command)
┌───────────┴───────────────────────────────▼───────────────────┐
│  ncm-core Actor（tokio 多线程 runtime）                         │
│  状态机 · 队列 · URL 解析/缓存 · 预取 · 数据库 · 事件广播       │
└───────▲───────────────────────────────┬───────────────────────┘
        │ AudioEvent(position/eof/err)   │ AudioCmd(Decoder/Pause/Seek)
┌───────┴───────────────────────────────▼───────────────────────┐
│  audio 线程（std::thread，独占 MixerDeviceSink + Player）       │
└───────────────────────────────────────────────────────────────┘
```

为什么必须是专用 OS 线程：`MixerDeviceSink` 内含 `cpal::Stream`，而 cpal 的 stream 持有平台回调指针，**不实现 `Send`**。最省事的正确做法是「谁创建谁持有」——在 `std::thread::spawn` 里建 sink，然后用通道收命令。

### 4.2 通道怎么选

| 用途 | 类型 | 理由 |
| --- | --- | --- |
| UI → core 命令 | `tokio::mpsc::channel(64)` | 有界，天然背压；命令本身需要 `await`（要查库/发请求） |
| core → audio 命令 | `std::sync::mpsc` 或 `crossbeam` | 音频线程不跑 async |
| 进度（高频、可丢） | `tokio::sync::watch<u64>` | 只保留最新值，合并不排队 |
| 事件（多订阅者） | `broadcast` + `ArcSwap<Snapshot>` | **快照才是真相**，broadcast 只当"变了"的提示；新起的窗口直接读快照，不会漏状态 |
| 播放器状态查询 | `ArcSwap<PlayerSnapshot>` | Tauri command 同步读，零锁 |

**不要**用 `broadcast` 当唯一状态源：`broadcast` 在接收慢时会丢消息，桌面歌词窗刚打开的那一帧就会缺状态。

### 4.3 播放状态机

```
Idle ──Play(track)──▶ Resolving ──url ok──▶ Buffering ──decoder ok──▶ Playing
                          │                     │                        │
                          │ url fail            │ decode fail            ├─pause──▶ Paused
                          ▼                     ▼                        │           │
                        Failed ◀────────────── Failed                    │◀─resume───┘
                          │                                                │
                          └── skip/retry ──▶ Resolving                    ├─eof──▶ (auto next) Resolving
                                                                           └─stop──▶ Idle
```

**不变量**（写进 `debug_assert!`，出 bug 时直接定位）：

1. 任一时刻最多一个 `Player` 实例在播；
2. 队列索引只由 core 线程推进，UI 只能提交意图（"下一首"），不能直接改索引；
3. 每次状态变更都带单调递增 `seq`，前端丢弃 `seq <= last_seen` 的事件；
4. `Playing` 状态下 `progress_at` 单调不减（除非发生 seek，此时 `pos` 允许回退并广播 `seeked` 事件）。

### 4.4 正确的最小骨架（rodio 0.23）

```rust
use rodio::{Decoder, DeviceSinkBuilder, Player, Source};
use std::io::{Read, Seek};
use std::sync::Arc;
use std::time::Duration;

enum AudioCmd {
    Load(Decoder<Box<dyn Read + Seek + Send + Sync>>),
    Pause,
    Resume,
    Seek(Duration),
    Volume(f32),
    Stop,
}

fn spawn_audio_thread(cmd_rx: std::sync::mpsc::Receiver<AudioCmd>) {
    std::thread::Builder::new()
        .name("ncm-audio".into())
        .spawn(move || {
            // 必须在同一个线程里创建并持有
            let mut sink = match DeviceSinkBuilder::open_default_sink() {
                Ok(s) => s,
                Err(e) => { eprintln!("no audio device: {e}"); return; }
            };
            sink.log_on_drop(false);

            let mixer = sink.mixer();
            let player = Player::connect_new(mixer);

            // 空闲 20ms 轮询：足够跟手，又不烧 CPU
            for cmd in cmd_rx.iter() {
                match cmd {
                    AudioCmd::Load(src) => {
                        player.append(src);       // 队列预挂载 = 无缝切歌
                        player.play();
                    }
                    AudioCmd::Pause  => player.pause(),
                    AudioCmd::Resume => player.play(),
                    AudioCmd::Seek(d) => { let _ = player.try_seek(d); }
                    AudioCmd::Volume(v) => player.set_volume(v.clamp(0.0, 1.0)),
                    AudioCmd::Stop => { player.stop(); }
                }
                // 注意：这里只做控制，进度由 core 侧定期 poll（见下）
                let _ = sink.config();
            }
        })
        .expect("spawn audio thread");
}
```

几个容易写错的点：

- **不要 `player.sleep_until_end()`**：那是"听完这首歌再返回"，会把控制循环卡死。用命令驱动 + core 侧轮询 `player.get_pos()`。
- **`get_pos()` 不是必须每帧读**：读它是 O(1)，但从 audio 线程跨线程推给 UI 需要通道；用 `Arc<AtomicU64>` 存毫秒，core 每 100ms 读一次再按 2Hz 推给前端最省。
- **单曲结束检测**：不要靠轮询 `player.empty()`——队列预挂载了下一首时它永远不会变 true。正确做法是包一层源：rodio 自带 `Done` 源（0.22 及以前用递减的 `Arc<AtomicUsize>`，master 上已改成回调，按你锁定的版本取用），或者自己实现 `Source` 适配器，在 `next()` 返回 `None` 时打点通知 core。
- **音量曲线**：`set_volume` 是线性幅度。滑杆位置应做感知映射（`v^2.5` 或 dB 分档），否则前半程几乎没变化。

### 4.5 反模式清单

- ❌ `Arc<Mutex<Player>>` 满天飞 —— Player 内部已有原子控制块，锁只会制造 UI 卡顿；
- ❌ 在 Tauri command 里 `await` 音频命令 —— command 应该只发消息立即返回；
- ❌ 在 core 里直接 `blocking_recv` —— 会饿死 tokio worker；
- ❌ 用 `player.get_pos()` 当歌词时间轴 —— 输出设备有 50-100ms 缓冲，需加固定偏移补偿（设置项里给用户 ±200ms 微调）。

---

## 5. 音频管线

### 5.1 流 + 缓存 + Seek（已实测）

实测一首 128kbps MP3：

```
HEAD  → 200, content-length: 5017644, content-type: audio/mpeg, accept-ranges: (空)
Range: bytes=1024-2047 → 206, content-range: bytes 1024-2047/5017644
```

结论：

1. **支持 Range，所以 `stream-download` + `Decoder` 的随机 Seek 可行**；
2. **但没有 `Accept-Ranges` 头**，不要用 header 判断能力（`stream-download` 会自己探测，别在外面加"支持性预检"）；
3. 直链 URL 形如 `https://m801.music.126.net/{YYYYMMDDHHmmss}/{hash}/{id}.mp3`，**有效期编码在路径时间戳里**，改一个字符就 403。

`stream-download` 0.24 的正确用法（注意：**0.24 只有 temp / memory / bounded / adaptive 四种存储**，没有"指定文件名"的持久化存储）：

```rust
use stream_download::{Settings, StreamDownload};
use stream_download::storage::temp::TempStorageProvider;

// 创建即开始后台下载 —— 这本身就是"预取"动作
let reader = StreamDownload::new_http(url, TempStorageProvider::default(), Settings::default()).await?;
let decoder = Decoder::new(reader)?;   // Decoder 需要 Read + Seek
```

推论（很重要，容易理解错）：

- **边下边播开箱即用**：临时文件由库自己管理，进程退出即清理；
- **"下载下来留在本地当缓存"不是它的事**：想要持久化音频缓存，要么实现自己的 `StorageProvider`（实现 `StorageReader` / `StorageWriter`），要么干脆走独立下载任务。这两件事不要混在一个抽象里。
- 创建 `StreamDownload` 时后台下载任务就已经开跑，所以"预取"= 提前 `new_http` 并把 `Decoder` 建好，而不是提前拿 URL。

### 5.2 预取与 URL 生命周期

- 响应里 `expi: 1200`，**就在 20 分钟后失效**。策略：缓存条目记 `expires_at = now + expi - 60`；播放取用前校验，过期就重新解析。
- 预取触发点：`position >= duration - 20s`（不是固定的 80%）——短歌 80% 太晚，长歌 80% 太浪费带宽。
- 预取动作分两步：① 解析下一首 URL；② **立刻 `StreamDownload::new_http` 建流**（库会马上在后台拉数据，等于把 TLS 握手和首包提前吃掉）；只存 URL 不建流等于没预取。
- 预取失败（VIP/版权/下架）时，把该曲标记 `unplayable`，`Eof` 时自动跳过并 toast，不要卡在"加载中"。
- 用户连续快速切歌时，用 `watch` 合并请求 + 单飞（`OnceCell` 按 song_id 去重），避免同时打 10 个 URL 请求触发风控。

### 5.3 无缝播放的真实边界

rodio 的 `Player` 队列在混音回调里连续取样本，**接缝处本身没有静音**。真正会听到缝的是：

| 来源 | 大小 | 处理 |
| --- | --- | --- |
| MP3 编码器 priming（LAME 头里的 delay/padding） | 常见 20-50ms | P2：解析 LAME tag，`SkipDuration` 裁掉；没有 LAME 头的文件无法修正 |
| 换源采样率不同（44.1k ↔ 48k） | 会重采样，有轻微失真 | rodio 0.22 已修队列混用的失真问题 |
| 下一首没准备好 | 秒级 | 靠 §5.2 的预取；未就绪时宁可留 200ms 静音，也别卡住当前曲 |

所以"无缝"是**工程目标**（预取到位 + 预挂载），不是库的开关。

### 5.4 增益与响度

响应里带了 `gain: -7.6179`、`peak: 1.0`（网易的响度归一化参数）。做法：

- 设置项"响度归一化"开启时：`player.set_volume(base * 10^(gain/20))`，再乘一个峰值保护 `min(1/peak, 1)`；
- 关闭时用 1.0，避免把用户原本的听感改掉（很多人不喜欢被"归一化"）。

这比引入 `ebur128` 自己分析省一整个模块——但要注明：`gain` 的参考基准是网易自己的，不是 ReplayGain 2.0 标准，跨客户端对比会有偏差。

### 5.5 频谱、EQ（P3，不进 v1）

- 频谱：在 `Player` 前面插一层 `Source` 适配器，把 f32 样本写进 `rtrb` 环形缓冲；独立线程 60Hz 取 2048 点做 Hann 窗 + `rustfft`，输出 32 段对数能量。**不要在音频回调里做 FFT**。
- EQ：同一位置插 5-10 个 `biquad`（peaking shelf），参数存配置。注意改参数要避免爆音（做 20ms 线性过渡）。

### 5.6 设备热插拔

`cpal` 没有跨平台"设备变更"事件。可行做法：

1. 每 2s 轮询 `cpal::default_host().default_output_device()` 的名字；
2. 变化时把旧 sink drop、重建、恢复到原 `position`（`try_seek`）；
3. 全程失败（拔了耳机又没别的设备）→ 状态置 `Paused` + 提示"未检测到输出设备"，绝不静默丢播放进度。

---

## 6. 协议层（全部实测过）

### 6.1 加密

两种都实现，默认 weapi：

| | weapi | eapi |
| --- | --- | --- |
| 算法 | AES-128-CBC × 2（预设 key + 随机 key）+ RSA 加密随机 key | AES-128-ECB，固定 key `e82ckenh8dichen8` |
| 载荷 | `params`(base64) + `encSecKey`(hex) | `params`(hex 大写)，内含 `path-36cd479b6b5-json-36cd479b6b5-md5` |
| 参数容忍度 | 宽 | 窄（实测多传 `total: true` 直接 404） |

RSA 那个"逆序"细节最容易写错：先把 16 字节随机 key 按字节反转，再按大端解释成整数，用 `010001` 指数做 `modpow`，结果左补零到 256 个 hex 字符。

**验收方式**：把 `node tools/ncm-crypto-probe.mjs --vectors` 的输出写进 Rust 的 `#[test]`，任何实现漂移都会红。黄金向量见附录 B。

### 6.2 会话与 Cookie

- 用 `reqwest::cookie::Jar` 自动管理 `Set-Cookie`；**不要手动拼 Cookie 头**。
- 首次启动生成并持久化 `deviceId`（16 位 hex），写入 Cookie：`os=pc; appver=2.10.12; deviceId=...; channel=netease;`。
- `__csrf` 从 jar 里读，POST 写操作必须带（`csrf_token` 字段）。
- 未登录先调 `/api/register/anonimous` 拿匿名 cookie，能显著降低风控概率。

### 6.3 二维码登录状态机

```
idle → 请求 unikey(/api/login/qrcode/unikey?type=1) → 渲染二维码
     → 轮询 client/login(key, type=1)：
        800 过期 → 重新取 unikey
        801 待扫描
        802 已扫描待确认
        803 成功    → Set-Cookie 落 jar，拉 /api/nuser/account/get 验身份
```

细节：二维码内容就是 `unikey` 本身；轮询间隔 1.5s；成功后必须校验 `account.id` 非空再切 UI 状态。**Cookie 落盘用系统凭据存储**（Windows Credential Manager / macOS Keychain / libsecret，`keyring` crate），不要明文写 `config.json`。

### 6.4 直链等级与降级

`/api/song/enhance/player/url/v1`，`level ∈ {standard, higher, exhigh, lossless, hires}`，`encodeType ∈ {mp3, flac, aac}`。

**必须写的一层包装**：

```rust
fn pick_url(resp) -> Result<Url> {
    let d = resp.data.first().ok_or(NoData)?;
    if d.url.is_none() || d.code != 200 {          // ← 内层码才是真相
        return Err(Unplayable { code: d.code, fee: d.fee });
    }
    if d.free_trial_info.is_some() { return Ok(Trial(d.url.unwrap())); }
    Ok(Full(d.url.unwrap()))
}
```

降级阶梯：用户偏好 → 逐级下降 `[lossless, exhigh, standard]`，每级失败都**重新请求**（不是猜 URL）；全失败则展示"该曲目暂无版权 / 需要会员"。`fee` 字段（0 免费 / 1 VIP / 8 低音质免费）可以用来提前提示，比播放失败再报错体验好。

### 6.5 歌词

`/api/song/lyric`（**不要**用 `/api/song/lyric/v1`：实测传 `cp/lv/kv` 为 0/false 会返回 `{"msg":"参数错误","code":400}`，甚至响应体重复拼接导致 JSON 解析失败），参数 `lv=-1, kv=-1, tv=-1, rv=-1, yv=-1`。

返回字段：`lrc` / `yrc` / `tlyric` / `romalrc` / `klyric`（`klyric` 是旧的 KTV 格式，忽略）。

**实测 YRC 结构**：

```
[28480,11820](28480,160,0)我(28640,420,0)带(29060,230,0)着…
 └行起点 └行时长  └词起点 └词时长 └flag(恒 0)
```

解析要点：

1. 行头 `[start_ms, duration_ms]`；
2. 每段 `(start_ms, duration_ms, flag)` 后紧跟该段文字（可能包含空格，歌词里的空格是排版，要保留）；
3. `duration_ms = 0` 的行是纯元信息（作词/作曲），不参与高亮；
4. 逐字进度 = 当前词片段内按 `(pos - word.start) / word.duration` 插值——这是"逐字"和"逐句"的唯一区别，CSS 渐变由前端做。

**回落策略**（必须实现，很多老歌没有 yrc）：`yrc` → `lrc`（整行一个"词片段"，按行时长线性推进）。翻译 `tlyric` 用行起点做最近匹配（±100ms），不要假设两轨行数完全一致。

### 6.6 限流、重试与风控

实测现象（都是真事）：

- 连续快速请求 URL 接口时，会返回**非 JSON**（`{"msg":"参数错误","code":400}` 重复拼接）；`JSON.parse` 直接抛错。
- 所以：**任何非 JSON / 非 200 响应都当作可重试错误**，而不是 `unwrap()`。
- 服务端没有返回"限流"的明确信号（HTTP 仍 200），只能靠客户端自律：全局令牌桶 8 rps + 100ms 抖动；URL 解析并发 ≤ 2；失败退避 1s/2s/4s，三次后降级。
- `-460` 是已知的风控码（触发后一段时间内请求都会被拒），遇到直接暂停所有请求 60s 并提示用户。

---

## 7. 数据层

### 7.1 Schema（SQLite，WAL）

```sql
PRAGMA journal_mode = WAL;
PRAGMA synchronous  = NORMAL;
PRAGMA foreign_keys = ON;

CREATE TABLE track (
  id         INTEGER PRIMARY KEY,          -- 网易 song id
  name       TEXT NOT NULL,
  artists    TEXT NOT NULL,                -- JSON 数组
  album_id   INTEGER,
  album_name TEXT,
  duration_ms INTEGER NOT NULL,
  cover_url  TEXT,
  fee        INTEGER DEFAULT 0,
  updated_at INTEGER NOT NULL
);

CREATE TABLE playlist (
  id INTEGER PRIMARY KEY, name TEXT NOT NULL, cover_url TEXT,
  track_count INTEGER DEFAULT 0, subscribed INTEGER DEFAULT 0,
  creator_id INTEGER, updated_at INTEGER
);

CREATE TABLE playlist_item (
  playlist_id INTEGER NOT NULL REFERENCES playlist(id) ON DELETE CASCADE,
  track_id    INTEGER NOT NULL REFERENCES track(id),
  position    INTEGER NOT NULL,
  PRIMARY KEY (playlist_id, position)
);

CREATE TABLE queue_state (            -- 单行，id 恒为 1
  id INTEGER PRIMARY KEY CHECK (id = 1),
  track_ids TEXT NOT NULL,            -- JSON 数组，保序
  cursor    INTEGER NOT NULL,
  position_ms INTEGER NOT NULL,
  repeat_mode TEXT NOT NULL DEFAULT 'off',   -- off | one | all
  shuffle   INTEGER NOT NULL DEFAULT 0,
  shuffle_order TEXT,                 -- JSON 数组；换歌单时重建
  updated_at INTEGER NOT NULL
);

CREATE TABLE play_history (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  track_id INTEGER NOT NULL, played_at INTEGER NOT NULL, ms_played INTEGER
);
CREATE INDEX idx_history_time ON play_history(played_at DESC);

CREATE TABLE lyric_cache (
  track_id INTEGER PRIMARY KEY, lrc TEXT, yrc TEXT, tlyric TEXT,
  fetched_at INTEGER NOT NULL
);

CREATE TABLE url_cache (              -- 不持久化也能跑，持久化是为了冷启动秒开
  track_id INTEGER NOT NULL, level TEXT NOT NULL,
  url TEXT NOT NULL, expires_at INTEGER NOT NULL, br INTEGER, gain REAL, peak REAL,
  PRIMARY KEY (track_id, level)
);

CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
```

设计取舍说明：

- `url_cache` 带 `expires_at`，冷启动时直接丢弃过期行（不用后台清理任务）。
- `queue_state` 存 JSON 数组而不是关联表：队列是有序列表、整体替换，用关联表反而要处理 position 重排的事务。
- 历史单独一张表而不是塞进队列表：它是无限增长的，需要独立的分页与裁剪（保留最近 5000 条）。

### 7.2 迁移

`PRAGMA user_version` + 一个 `match version { 0 => v1(), 1 => v2(), ... }`。每次迁移包在一个事务里；**迁移前自动 `VACUUM INTO 'backup-<date>.db'`**，失败可回滚。比引 migration 框架少 300 行依赖。

### 7.3 缓存容量

- 音频缓存目录：默认上限 2GB，LRU 按 `atime` 淘汰（SQLite 记一张 `cache_file(path, size, last_used)`）。
- 封面：统一落盘成 300×300 的 webp，避免每次列表滚动都走网络；失败有占位图。
- 歌词：永久缓存（文本很小），但带 `fetched_at`，超过 90 天可后台刷新。

### 7.4 凭据

Cookie / `MUSIC_U` 只进 `keyring`，DB 里只存 `"keyring:netease"` 这样的引用。导出诊断包时**自动排除**该字段。

---

## 8. 前端与 IPC

### 8.1 状态同步：快照 + 差分事件 + 本地插值

```
前端启动 → invoke("get_snapshot") → 完整 PlayerSnapshot
         → listen("player://event") → 增量（TrackChanged / StateChanged / Seeked）
         → listen("player://position") → 2Hz {pos_ms, at: monotonic_ms}
```

前端维护 `pos = pos_ms + (now - at) * rate`，用 `requestAnimationFrame` 更新进度条与歌词——**这样即使 IPC 掉帧，进度条也是丝滑的**。暂停时把 rate 置 0。

这一条是对早期方案最实质的改进：100ms 一次 IPC 在 Tauri 下每个事件都要序列化 + 跨进程投递，播放时 CPU 会莫名其妙地高。

### 8.2 类型生成

`ncm-ipc` 里所有 `Command` / `Event` / `Snapshot` 加 `#[derive(TS)]`，构建时 `cargo test export_bindings` 生成 `src/lib/bindings.ts`。前端不再手写类型，字段改名会立刻编译报错。

### 8.3 桌面歌词窗

```ts
const w = new WebviewWindow('lyrics', {
  url: '/lyrics',
  decorations: false, transparent: true, alwaysOnTop: true,
  skipTaskbar: true, resizable: false, shadow: false,
});
await w.setIgnoreCursorEvents(true);   // 锁定 = 鼠标穿透
```

要点：

- **默认锁定（穿透）+ 全局快捷键解锁**，否则用户拖不动窗口；
- 窗口位置/字号/颜色持久化，显示器拔掉后按 `availableMonitors` 裁剪回可视区；
- Windows 想加亚克力用 `window-vibrancy`；macOS 透明窗需要 `macOSPrivateApi`（**上架 App Store 会被拒**，自签名分发无碍）；Linux 依赖合成器，检测不到就退化成半透明纯色底；
- 歌词渲染用 CSS `background-clip: text` 做逐字高亮，按 `(pos - word.start)/word.duration` 更新一个 CSS 变量，不要每帧重建 DOM。

### 8.4 托盘 / 媒体键 / 单实例

- 托盘：`tauri::tray::TrayIconBuilder` + 菜单（播放/暂停、上一首/下一首、显示主窗口、退出）。
- 媒体键：`souvlaki` 0.9.3。**回调在 OS 线程触发，只能往 core 发命令**；Windows 需要传窗口句柄（`PlatformConfig.hwnd`）；封面按平台传（Windows 用本地文件路径，MPRIS 用 `file://` URI，macOS 传字节）。
- 单实例：`tauri-plugin-single-instance`，把第二次启动的参数转发到已有实例。
- 防休眠：播放中调 `keepawake`，暂停释放。

---

## 9. 工程化

### 9.1 Profile

```toml
[profile.release]
lto = "fat"
codegen-units = 1
panic = "abort"
strip = true
opt-level = 3
```

`opt-level = "s"` 大约能再省 15% 体积，但解码与音频处理会慢一些——播放器不值得省这 3MB。

### 9.2 CI 矩阵

| 平台 | 依赖 | 产物 |
| --- | --- | --- |
| Windows | MSVC 工具链 + WebView2 引导 | `.msi` / `.exe`（NSIS），签名用 EV 证书或自签名 |
| macOS | Xcode CLT | universal `.dmg`，公证 (`notarytool`) |
| Linux | `libwebkit2gtk-4.1-dev libasound2-dev libayatana-appindicator3-dev` | `.deb` / AppImage / `.rpm` |

用 `tauri-apps/tauri-action`，配 `Swatinem/rust-cache` + `sccache`。首次冷编译 ~15 分钟，缓存后 ~3 分钟。

### 9.3 可观测性

- `tracing` + `tracing-appender` 滚动文件（按天，保留 7 天，上限 50MB）；
- panic hook 写 `crash-<ts>.log`（附 backtrace）；
- "导出诊断包"：zip（日志 + 版本 + 设备信息 + 最近 100 条错误），**不含 Cookie/token**；
- 默认零遥测。要埋点也必须 opt-in 且脱敏。

### 9.4 测试

| 层 | 方式 | 验收标准 |
| --- | --- | --- |
| 加密 | 黄金向量 `#[test]`（附录 B） | params/encSecKey 逐字符相等 |
| 歌词 | 真实样本 fixture + 快照测试 | LRC/YRC/tlyric 三种输入都能产出正确的 `LyricLine` |
| 队列 | `proptest` 随机操作序列 | 不变量：不重复播、shuffle 全遍历、删除当前曲后的 cursor 正确 |
| URL 缓存 | 注入假时钟 | 过期前 60s 必定刷新 |
| API | `wiremock` + 录制的响应夹具 | 内层 code=404 走降级、非 JSON 走重试 |
| 解码 | rodio `default-features=false, features=["symphonia-all"]`，无音频设备 | 解出的样本数与 `total_duration` 误差 ≤ 1 帧 |
| UI | 手工清单（每版发） | 见 §10 验收 |

注意最后一行：**解码测试不需要声卡**，这让 CI 能跑音频相关的回归，是这类项目最容易被忽略的一环。

---

## 10. 里程碑（每一步都能演示）

| 里程碑 | 内容 | 验收（可观察） | 估时 |
| --- | --- | --- | --- |
| **M0 骨架** | workspace + 加密 + 搜索 | `cargo test` 全绿；CLI 能搜出 10 首歌 | 2-3 天 |
| **M1 出声** | 直链 → 解码 → 播放 | 点一首歌 < 1.5s 出声；暂停/继续/seek/音量可用；进度条 60fps 不抖 | 3-5 天 |
| **M2 像播放器** | 队列、循环/随机、预取无缝、LRC 歌词、媒体键、托盘 | 连播 10 首无卡顿、切歌无静音；锁屏媒体面板能显示且能控 | 1-2 周 |
| **M3 完整** | 登录、歌单、每日推荐、收藏、逐字歌词、桌面歌词窗、设置、缓存管理 | 二维码登录成功；歌单 500 首滚动不卡；桌面歌词逐字高亮准 | 2-3 周 |
| **M4 打磨** | 下载离线、本地音乐、自动更新、i18n、崩溃上报、安装包 | 三平台安装包可装可卸；断网重连恢复播放 | 2 周 |

**先做 M0 的原因**：加密与搜索是唯一"做错了整条链路都得重来"的部分，必须最早验证；而 UI 是最容易返工的部分，越晚定型越省钱。

---

## 11. 风险与明确不做的事

| 风险 | 影响 | 缓解 |
| --- | --- | --- |
| 网易改接口/加风控 | 全站不可用 | 协议层隔离在 `ncm-api`；端点表数据化；保留 weapi/eapi 双通道 |
| 直链策略收紧（试听/降码率） | 体验退化 | 等级阶梯 + 明确文案，不假装能播 |
| `souvlaki` 在 Linux 无 DBus 环境 | 媒体键失效 | 降级为只支持托盘，不阻塞启动 |
| Tauri 透明窗在 Win10 1809 之前 | 桌面歌词黑底 | 检测版本，退化为不透明窗口 |
| 上传/分享类接口需要 `__csrf` | 写操作报错 | 统一在 client 层注入 |

**不做**（不是懒，是边界）：

1. **不实现任何绕过付费/版权的内容替换**（包括 UnblockNeteaseMusic 那类"音源替换"）——这是侵权，不是技术问题；
2. 不做 DRM 解密；
3. 不做服务端中转（无账号体系，纯本地）；
4. 不内置任何"共享会员"能力。

---

## 附录 A：实测事实与复现

```bash
node tools/ncm-crypto-probe.mjs            # 线上实测 5 项
node tools/ncm-crypto-probe.mjs --vectors  # 黄金向量
```

2026-11-06 实测结果：

| 项 | 结果 |
| --- | --- |
| `weapi` 搜索 | HTTP 200，`code: 200` |
| `eapi` 搜索 | HTTP 200，`code: 200` |
| 直链 | 外层 `code: 200`，内层 `data[0].code: 200`，`expi: 1200`，`br: 128000`，`gain: -7.6179` |
| Range | `206`，`content-range: bytes 1024-2047/5017644`；`accept-ranges` 头为空 |
| 歌词 | `yrc` 41 行可解析；《晴天》无 yrc → 必须回落 lrc |

## 附录 B：加密黄金向量

输入固定、输出确定，直接写进 `crates/ncm-api/src/crypto.rs` 的 `#[cfg(test)]`：

```json
{
  "weapi": {
    "input": { "s": "晴天", "type": 1, "limit": 1, "offset": 0 },
    "nonce": "0123456789abcdef",
    "params": "/6VzyqxiY4GeAKvcAu0k7VotTQs/KAHmIOgt0kQNJtd0aNfYEzP1vpLlAjOY1rvi8iew6O1gqTu7pP+cM4pZfm4bufN9atZNQgz4kPdD9HQUEwCH9eBc8S/h6qnYOOmC",
    "encSecKey": "35701388baf89fed412e11269b9c76625d095ecaf17f03fa018abe19ea2d38b949debf242ee39a71ca1f6cda71b1b86a45aa909ee27f7e78e267d34e732f0de948206c3340a788d0003372183e2f753c1f78b66ac23d134ac1fc9b993156520ea826b8aa89a962d4491b4b8d7e08738e1da9b07aa39bf4a7ef0b1c210728cd52"
  },
  "eapi": {
    "path": "/api/search/get",
    "input": { "s": "晴天", "type": 1, "limit": 1, "offset": 0 },
    "params": "1AF0E93B0E3EA03CE4E7F1B6AD7BD32BC198D7B70109AB343E0FC0C4A8F27C965817DFB6318A4DBB7F44CD9CD14FDD2CB3B194245FFB476BA70C2CA92927CDE240245A90B3801278F541BEC912192AFA47B491F481FB0EE6D0072EEEBFE706EFEDF635D2FF8DF073E621F9959F3ED0C5B58DF95C10D3661D97746BADA7DC9BBF"
  }
}
```

---

## 附录 C：当前环境（2026-11-06 实测）

| 项 | 状态 |
| --- | --- |
| Rust | 1.99.1，默认 `stable-x86_64-pc-windows-msvc`；`-gnu` 作为备用仍在 |
| VS Build Tools | 2022 17.14（MSVC 14.44.35207 + Windows SDK 10.0.26100），`link.exe` 就位 |
| MinGW | winlibs GCC 16.2 → `%USERPROFILE%\mingw-toolchain\mingw64\bin`，仅 GNU 目标需要 |
| Node | 24.11 / npm 11.6，前端用 npmmirror |
| 管理员 | 账户在 Administrators 组但令牌是 deny-only，安装器靠 UAC 提权 |

踩过并已解决的坑（详见 README「已知环境坑」）：

1. `windows-sys` 在 GNU 目标下走 raw-dylib，链接要 `dlltool`；rustup 的 `llvm-tools` 里没有 `llvm-dlltool`，只能装 MinGW。切 MSVC 后不再需要。
2. DSH 沙箱是低完整性令牌：`cargo`/`curl` 的 schannel 拿不到 TLS 凭据，`npm` 写不了自己的 cache，Vite 的 `child_process` 探测会 `spawn EPERM`。沙箱外都正常。
3. 下载源：`rsproxy.cn` 约 11MB/s，`static.rust-lang.org` 约 50KB/s，GitHub 直连 56KB/s、`ghfast.top` 前缀约 3MB/s。

M0 验收（实际跑过）：`cargo test -p ncm-api` 4/4（weapi/eapi 与 Node 黄金向量逐字符一致，MSVC 与 GNU 两套工具链下都过）；`ncm-cli search 晴天` 3 条；`ncm-cli url 26259733` 拿到 320kbps 直链 + `expi=1200` + `gain=-8.0564`；`ncm-cli lyric 1974443814` 拿到 41 行 yrc。
