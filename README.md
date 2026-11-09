# Manbamusic

一个用 Rust 写的第三方网易云音乐桌面播放器。**Tauri v2 + Svelte 5 薄前端 + 厚 Rust 后端**：协议、音频、歌词、系统集成全在 Rust，前端只做渲染和交互。

> 个人自用项目，只调用官方 Web 接口，不涉及音源替换、DRM 破解或账号共享。

## 设计方法

这个项目不是先画界面再补功能，而是按下面几条原则推进的。

**1. 厚度放在 Rust 侧**

业务逻辑一律沉到 `crates/` 下的 `ncm-*`：协议、加密、音频、歌词、播放器状态机都在 Rust 里。Tauri 外壳只做窗口、IPC 和状态装配，前端是「只读状态 + 发命令」的薄层。

好处是任何一块都能单独替换：`ncm-api` 不认识音频，`ncm-audio` 不认识网易云，`ncm-core` 不认识 Tauri。GUI 从 Tauri 换掉，业务代码一行不用动。

**2. 协议先实测，再写实现**

接任何接口前先写探针脚本打真实请求，确认字段、加密方式、过期时间、权限行为，再落 Rust 代码。比如直链响应外层 `code` 恒为 200、真实结果在内层 `data[0].code`；`expi=1200`；`fee=8` 表示标准音质免费而不是试听——这些都是实测出来的，和文档写的、和别人方案里写的不一样。

**3. 播放器是一个 actor**

音频设备对象（`MixerDeviceSink`）不是 `Send`，所以整条播放链路独占一个线程：外部只能发命令（播放/暂停/跳转/音量），只能读状态快照。UI 永远碰不到音频对象，也就不会因为前端卡顿影响出声。

**4. 用缓存换随机访问**

流式播放的问题是不能往回拖。解决办法是渐进缓存：边下边写本地文件，播放端读一个「长度已知、读到未下载区域会等待」的视图，解码器于是获得随机访问能力——**缓存到哪就能拖到哪**，没有缓存的部分短暂等待，和官方客户端手感一致。

**5. 视觉收敛**

界面目标只有一个：干净、原生、不端着。具体做法是完全使用系统字体栈（不打包字体）、尽量不画分隔线、用留白和层级区分而不是靠边框，主题只从 4 个种子色自动派生出一整套变量。

**6. 每一步都跑端到端验证**

改完不是「看起来没问题」就算完：纯逻辑补单测，协议用探针实测，前端 `svelte-check` + `vite build`，桌面端用内置冒烟通道在 WebView 里真实调用命令并回传结果。

## 功能

**播放**

- 搜索歌曲（云搜索接口，带封面/专辑/时长）
- 队列播放：点任意一首从那里开始排队，播完自动下一首；列表支持单击选中、双击播放
- 上/下一首、列表循环、单曲循环、随机
- 无缝切歌：下一首提前预挂载进解码队列，切歌不等网络
- 直链缓存（按接口 `expi` 过期），切歌不重复请求
- 在线音质选择（标准/较高/极高/无损），未登录 VIP 曲目识别为试听
- 进度条与音量：点击、拖动、方向键、滚轮、静音记忆
- 音量以应用状态为准，拖动 250ms 节流落盘

**缓存**

- 渐进缓存：边下边播，已下载部分前后拖都瞬时
- 整首下完写入 `<data>/cache/tracks/<id>.audio`，再次播放直接读本地
- 上限 1 GB / 150 首，按最后播放时间淘汰

**界面**

- 自绘无边框窗口 + 34px 标题栏浮层（窗口按钮、拖动）
- 6 套主题 + 浅色模式，全部由 4 个种子色派生
- 虚拟滚动列表：1000 首以上只渲染视口内约 20 行
- 全窗口「正在播放」页：大封面 + 控制簇 + 歌词
- 歌词：YRC 逐字优先、回落 LRC、合并翻译，按播放位置滚动高亮
- 桌面歌词窗：无边框、透明置顶、点击穿透
- 待播清单抽屉：按播放顺序展开，可直接跳转
- 设置页：在线音质、关闭窗口行为（退出 / 最小化到托盘）

**账号与内容**

- 扫码登录，`MUSIC_U` 落盘，下次启动自动恢复
- 我的歌单（含官方「我喜欢的音乐」，按 `specialType=5` 判定）
- 收藏红心：与网页端同一接口，双向同步
- 每日推荐、漫游（私人 FM，播到队尾自动续歌）、私人雷达

**本地音乐**

- 选择文件夹递归扫描，支持 mp3 / flac / m4a / aac / wav / ogg / opus / wma / ape / aiff
- 读标签（标题/歌手/专辑/时长），读不到就退回文件名
- 直接读本地文件播放，不经过网络与缓存
- 目录列表存在 `settings.json`，可多目录、可移除

**持久化**

- 重启后恢复音量、循环/随机模式、整条播放队列与当前位置
- 状态存 `<data>/state.json`，设置为 `<data>/settings.json`

## 技术栈

| 层 | 选型 |
| --- | --- |
| 桌面外壳 | Tauri v2 |
| 前端 | Svelte 5（runes）+ Vite 8 + TypeScript 5.9 |
| 音频 | rodio 0.23（symphonia 解码）+ reqwest 渐进下载 |
| 异步运行时 | tokio |
| 系统集成 | souvlaki（媒体键 / SMTC）、Tauri 托盘 |
| 协议 | weapi（AES-CBC×2 + RSA）/ eapi，自维护 Cookie 表 |
| 标签解析 | lofty |

## 快速开始

需要 Rust（MSVC 工具链）与 Node 20+。

```bash
# 1. 装前端依赖
cd apps/desktop && npm install

# 2. 构建前端（产物会被嵌进二进制）
npm run build

# 3. 跑桌面端
cd ../.. && cargo run -p manbamusic-desktop --features custom-protocol
```

只调样式时可以用浏览器看 UI（走 mock 数据）：

```bash
cd apps/desktop && npm run dev
```

### 常用命令

```bash
cargo test --workspace                     # 单元测试
cargo clippy --workspace --all-targets -- -D warnings
cd apps/desktop && npm run check           # svelte-check
cargo run -p ncm-cli -- search 晴天 --limit 5
cargo run -p ncm-cli -- url 26259733
```

### 数据目录

默认 `%LOCALAPPDATA%\Manbamusic`（WebView 数据、音频缓存、`login.json`、`state.json`、`settings.json`）。

写不进去时依次回退到 exe 同级 `Manbamusic-data`、`%TEMP%\Manbamusic`，并在 exe 旁写 `startup.log` 记录回退原因。也可以用 `MANBAMUSIC_DATA_DIR` 直接指定。

## 目录结构

```
crates/
  ncm-api/       协议层：weapi/eapi 加密、Cookie 会话、搜索/直链/歌词/登录/红心/推荐/漫游
  ncm-audio/     音频管线：渐进缓存、流式回退、rodio 解码、缓存淘汰
  ncm-core/      播放器 actor（命令通道 + 状态快照 + 播完事件 + 跳转）
  ncm-lyrics/    歌词解析：LRC / YRC 逐字 / 翻译合并
  ncm-platform/  系统媒体键（souvlaki）
apps/
  desktop/       Tauri 外壳（命令与队列）+ Svelte 前端
  cli/           命令行小工具（协议与播放探针）
```

依赖方向严格单向：`desktop/cli → ncm-api / ncm-audio / ncm-core / ncm-lyrics / ncm-platform`，`ncm-core → ncm-audio`。

## 架构要点

- **三线程**：WebView（渲染）、tokio（网络与命令）、独占音频线程（音频设备对象不是 `Send`）。外部只能给播放器发命令、读快照。
- **队列三层**：`tracks` 是曲目池，`order` 是播放顺序（随机就是打乱后的顺序），`position` 是 order 里的下标；漫游模式下额外有「续歌」策略，走到队尾再取一批。
- **无缝切歌**：预挂载状态下 `player.empty()` 恒为 false，换曲判断依赖 `TrackTap` 上报的 `TrackEnded`。
- **IPC**：前端每 400ms 轮询播放快照，拖动时用本地预览值，松手后等快照追上目标位再交还，避免进度条闪回。
- **主题**：组件只写 `var(--token)`，换主题 = 改 `:root` 变量，不重新渲染。

更完整的协议细节、线程模型与踩坑记录见 [ARCHITECTURE.md](ARCHITECTURE.md)。

## 已知限制

- 单个歌单最多取 5000 首（接口分页上限内的保护值）。
- 尚未做下载/离线、歌单编辑。
- 桌面歌词窗固定深色文字（不跟随主题）。
- 自定义主题与界面偏好仍存在 WebView 的 localStorage，尚未落盘到数据目录。
- 无 CI、无自动打包与签名。

## 声明

仅供个人学习与自用。所有音乐内容版权归网易云音乐及对应版权方所有，本项目不提供任何音源，也不绕过付费与版权限制。使用产生的任何后果由使用者自行承担。

## 许可证

尚未选择许可证。如需使用请先联系作者。
