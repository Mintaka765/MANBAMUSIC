<script lang="ts">
  import { onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import QRCode from 'qrcode';
  import Lyrics from './lib/Lyrics.svelte';
  import Icon from './lib/Icon.svelte';
  import NowPlaying from './lib/NowPlaying.svelte';
  import PlayerBar from './lib/PlayerBar.svelte';
  import SearchBar from './lib/SearchBar.svelte';
  import ThemePicker from './lib/components/ThemePicker.svelte';
  import QueuePanel from './lib/QueuePanel.svelte';
  import SettingsPanel from './lib/SettingsPanel.svelte';
  import TrackList from './lib/TrackList.svelte';
  import { themeStore } from './lib/theme/store.svelte';
  import {
    getLyric,
    loginQrPoll,
    loginQrStart,
    logout,
    myPlaylists,
    myProfile,
    nextTrack,
    pausePlayer,
    playerSnapshot,
    queueItems,
    playlistTracks,
    playQueue,
    previousTrack,
    addLocalFolder,
    dailySongs,
    localFolders,
    removeLocalFolder,
    scanLocal,
    playRadio,
    radarTracks,
    queueState,
    resumePlayer,
    searchTracks,
    seekPlayer,
    setRepeat,
    setVolume,
    likeSong,
    likedSongs,
    toggleLyrics,
    toggleShuffle,
  } from './lib/api';
  import type { LyricTrack, PlayerSnapshot, Playlist, Track, UserProfile } from './lib/types';

  let query = $state('');
  let tracks = $state<Track[]>([]);
  let listTitle = $state('');
  let loading = $state(false);
  let error = $state<string | null>(null);
  let searched = $state(false);
  let current = $state<Track | null>(null);
  /** 已喜欢的曲目 id；红心状态以服务端为准，本地只做乐观更新 */
  let likedIds = $state<Set<number>>(new Set());

  async function toggleLike() {
    const track = current;
    if (!track) return;
    const next = !likedIds.has(track.id);
    // 先改本地，再发请求；失败回滚
    const optimistic = new Set(likedIds);
    if (next) optimistic.add(track.id);
    else optimistic.delete(track.id);
    likedIds = optimistic;
    try {
      await likeSong(track.id, next);
    } catch {
      const revert = new Set(likedIds);
      if (next) revert.delete(track.id);
      else revert.add(track.id);
      likedIds = revert;
    }
  }
  let lyric = $state<LyricTrack | null>(null);
  let repeat = $state<'off' | 'all' | 'one'>('off');
  let shuffle = $state(false);
  let lyricsOpen = $state(false);
  let stageOpen = $state(false);
  let settingsOpen = $state(false);
  let queueOpen = $state(false);
  let settingsPageOpen = $state(false);

  const appWindow = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window ? getCurrentWindow() : null;
  const minimizeWindow = () => void appWindow?.minimize();
  const toggleMaximize = () => void appWindow?.toggleMaximize();
  const closeWindow = () => void appWindow?.close();



  let profile = $state<UserProfile | null>(null);
  let playlists = $state<Playlist[]>([]);
  let showLogin = $state(false);
  let showThemes = $state(false);
  let qrImage = $state('');
  let qrStatus = $state<'idle' | 'waiting' | 'scanned' | 'expired' | 'success'>('idle');
  let pollTimer: ReturnType<typeof setInterval> | null = null;

  let snapshot = $state<PlayerSnapshot>({
    state: 'idle',
    positionMs: 0,
    durationMs: 0,
    volume: 0.8,
    error: null,
    trackId: null,
    trialSecs: null,
  });

  const nav = [
    { key: 'search', label: '搜索', icon: 'song' as const, hint: '歌曲 · 歌手 · 歌单' },
    { key: 'discover', label: '发现音乐', icon: 'home' as const, hint: '每日推荐' },
    { key: 'radio', label: '漫游', icon: 'radio' as const, hint: '私人 FM，无限续播' },
    { key: 'radar', label: '私人雷达', icon: 'radar' as const, hint: '每天更新的雷达歌单' },
    { key: 'library', label: '我的音乐', icon: 'heart' as const, hint: '我喜欢的音乐' },
    { key: 'local', label: '本地音乐', icon: 'album' as const, hint: '扫描本地文件' },
  ];
  let active = $state('search');

  // 已经在本次会话里查过的曲目，避免重复请求（非响应式）
  const likeChecked = new Set<number>();

  // 当前曲目变化时补查一次红心状态：恢复队列、点歌单播放都不会经过列表同步
  (() => {
    const id = current?.id;
    if (!id || likeChecked.has(id)) return;
    likeChecked.add(id);
    let stale = false;
    likedSongs([id])
      .then((liked) => {
        if (stale || liked.length === 0) return;
        const next = new Set(likedIds);
        for (const value of liked) next.add(value);
        likedIds = next;
      })
      .catch(() => {
        likeChecked.delete(id);
      });
    return () => {
      stale = true;
    };
  });


  // 漫游模式下队列会在后端自动续歌：当前曲目变化时同步一下列表
  $effect(() => {
    const id = current?.id;
    if (active !== 'radio' || !id) return;
    void queueItems()
      .then((data) => {
        tracks = data.tracks;
      })
      .catch(() => {});
  });

  onMount(() => {
    themeStore.init();

    const timer = setInterval(async () => {
      try {
        snapshot = await playerSnapshot();
        const queue = await queueState();
        if (queue?.currentId != null && queue.currentId !== current?.id) {
          const next = queue.tracks.find((item) => item.id === queue.currentId);
          if (next) current = next;
        }
        if (queue) {
          repeat = queue.repeat;
          shuffle = queue.shuffle;
        }
      } catch {
        // 后端还没起来时静默
      }
    }, 400);

    (async () => {
      profile = await myProfile();
      if (profile) playlists = await myPlaylists();
    })();

    return () => {
      clearInterval(timer);
      if (pollTimer) clearInterval(pollTimer);
    };
  });

  // 当前曲目变化 → 拉一次歌词
  $effect(() => {
    const track = current;
    if (!track) {
      lyric = null;
      return;
    }
    let cancelled = false;
    getLyric(track.id)
      .then((value) => {
        if (!cancelled) lyric = value;
      })
      .catch(() => {
        if (!cancelled) lyric = null;
      });
    return () => {
      cancelled = true;
    };
  });

  async function runSearch() {
    if (!query.trim()) return;
    loading = true;
    error = null;
    try {
      tracks = await searchTracks(query);
      listTitle = `搜索「${query}」`;
      searched = true;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      loading = false;
    }
  }

  /** 每日推荐按天缓存：同一天内来回切导航不重复请求 */
  let dailyCache: { date: string; tracks: Track[] } | null = null;
  /** 私人雷达当次会话内不重复拉（歌单每天更新，重开应用才刷新） */
  let radarCache: Track[] | null = null;
  /** 本地音乐 */
  let localDirs = $state<string[]>([]);
  let localScanning = $state(false);
  let localCache: Track[] | null = null;

  function todayKey() {
    const now = new Date();
    return `${now.getFullYear()}-${now.getMonth() + 1}-${now.getDate()}`;
  }

  async function loadDaily() {
    error = null;
    searched = true;
    const key = todayKey();
    listTitle = `每日推荐 · ${new Date().getMonth() + 1} 月 ${new Date().getDate()} 日`;

    if (dailyCache && dailyCache.date === key) {
      tracks = dailyCache.tracks;
      return;
    }

    tracks = [];
    listLoading = true;
    try {
      const list = await dailySongs();
      if (list.length === 0) {
        error = '每日推荐需要登录后才能获取，先去左下角「设置」扫码登录。';
        return;
      }
      dailyCache = { date: key, tracks: list };
      tracks = list;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      listLoading = false;
    }
  }

  /** 侧栏导航：发现音乐 → 每日推荐；我的音乐 → 我喜欢的音乐 */
  async function selectNav(key: string) {
    active = key;
    if (key === 'discover') {
      await loadDaily();
      return;
    }
    if (key === 'radio') {
      error = null;
      searched = true;
      listTitle = '漫游 · 私人 FM';
      tracks = [];
      listLoading = true;
      try {
        await playRadio();
        const data = await queueItems();
        tracks = data.tracks;
      } catch (cause) {
        error = cause instanceof Error ? cause.message : String(cause);
      } finally {
        listLoading = false;
      }
      return;
    }

    if (key === 'radar') {
      error = null;
      searched = true;
      listTitle = '私人雷达';
      if (radarCache) {
        tracks = radarCache;
        return;
      }
      tracks = [];
      listLoading = true;
      try {
        const list = await radarTracks();
        if (list.length === 0) {
          error = '私人雷达需要登录后才能获取。';
          return;
        }
        radarCache = list;
        tracks = list;
      } catch (cause) {
        error = cause instanceof Error ? cause.message : String(cause);
      } finally {
        listLoading = false;
      }
      return;
    }

    if (key === 'local') {
      error = null;
      searched = true;
      listTitle = '本地音乐';
      try {
        localDirs = await localFolders();
        if (localDirs.length === 0) {
          tracks = [];
          localCache = [];
          return;
        }
        if (localCache) {
          tracks = localCache;
          return;
        }
        await rescanLocal();
      } catch (cause) {
        error = cause instanceof Error ? cause.message : String(cause);
      }
      return;
    }

    if (key !== 'library') return;
    try {      const lists = await myPlaylists();
      // 官方「我喜欢的音乐」specialType=5；账号里可能有多张历史遗留，取曲目最多的那张
      const liked =
        lists
          .filter((item) => item.specialType === 5)
          .sort((a, b) => b.trackCount - a.trackCount)[0] ??
        lists.find((item) => item.name === '我喜欢的音乐') ??
        lists[0];
      if (liked) await openPlaylist(liked);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  /** 本地音乐：重新扫描 */
  async function rescanLocal() {
    localScanning = true;
    listLoading = true;
    try {
      const list = await scanLocal();
      localCache = list;
      tracks = list;
      listTitle = `本地音乐`;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      localScanning = false;
      listLoading = false;
    }
  }

  /** 本地音乐：添加目录 */
  async function pickLocalFolder() {
    try {
      localDirs = await addLocalFolder();
      localCache = null;
      if (localDirs.length > 0) await rescanLocal();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  /** 本地音乐：移除目录 */
  async function dropLocalFolder(dir: string) {
    try {
      localDirs = await removeLocalFolder(dir);
      localCache = null;
      if (localDirs.length > 0) {
        await rescanLocal();
      } else {
        tracks = [];
        localCache = [];
      }
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  /** 本次会话内的歌单缓存：再点同一个歌单立刻出列表 */
  const playlistCache = new Map<number, Track[]>();
  const prefetching = new Set<number>();
  let listLoading = $state(false);

  /** 鼠标悬停就预取：服务端一次要 1.3s 左右，提前开始能省掉大半等待 */
  function prefetchPlaylist(id: number) {
    if (playlistCache.has(id) || prefetching.has(id)) return;
    prefetching.add(id);
    playlistTracks(id)
      .then((list) => playlistCache.set(id, list))
      .catch(() => {})
      .finally(() => prefetching.delete(id));
  }

  async function openPlaylist(playlist: Playlist) {
    error = null;
    listTitle = `歌单 · ${playlist.name}`;
    // 先切视图，别让用户对着旧列表干等
    searched = true;
    tracks = [];

    const cached = playlistCache.get(playlist.id);
    if (cached) {
      tracks = cached;
      return;
    }

    listLoading = true;
    try {
      const list = await playlistTracks(playlist.id);
      playlistCache.set(playlist.id, list);
      tracks = list;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      listLoading = false;
    }
  }

  async function play(track: Track, index: number) {
    current = track;
    try {
      await playQueue(tracks, index);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  async function step(direction: 'next' | 'previous') {
    try {
      if (direction === 'next') await nextTrack();
      else await previousTrack();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  type PlayMode = 'off' | 'all' | 'one' | 'shuffle';

  /** 把「循环模式 + 随机」合成一个四态：顺序 → 列表循环 → 单曲循环 → 随机 */
  function currentMode(): PlayMode {
    if (shuffle) return 'shuffle';
    if (repeat === 'one') return 'one';
    if (repeat === 'all') return 'all';
    return 'off';
  }

  async function cyclePlayMode() {
    const order: PlayMode[] = ['off', 'all', 'one', 'shuffle'];
    const next = order[(order.indexOf(currentMode()) + 1) % order.length];
    try {
      if (next === 'shuffle') {
        // 随机播放按「列表循环」的语义走：一轮放完继续随机，不会停
        await toggleShuffle(true);
        const queue = await setRepeat('all');
        if (queue) {
          repeat = queue.repeat;
          shuffle = queue.shuffle;
        }
      } else {
        await toggleShuffle(false);
        const queue = await setRepeat(next);
        if (queue) {
          repeat = queue.repeat;
          shuffle = queue.shuffle;
        }
      }
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  async function flipLyrics() {
    lyricsOpen = !lyricsOpen;
    try {
      await toggleLyrics(lyricsOpen);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  async function openLogin() {
    showLogin = true;
    await refreshQr();
  }

  async function refreshQr() {
    qrStatus = 'waiting';
    qrImage = '';
    try {
      const key = await loginQrStart();
      // App 扫的是登录链接，不是裸 key：官方客户端的二维码内容就是
      // https://music.163.com/login?codekey=<unikey>
      const content = `https://music.163.com/login?codekey=${key}`;
      qrImage = await QRCode.toDataURL(content, {
        margin: 1,
        width: 220,
        errorCorrectionLevel: 'M',
      });

      if (pollTimer) clearInterval(pollTimer);
      pollTimer = setInterval(async () => {
        try {
          const status = await loginQrPoll(key);
          qrStatus = status;
          if (status === 'success') {
            if (pollTimer) clearInterval(pollTimer);
            pollTimer = null;
            profile = await myProfile();
            playlists = await myPlaylists();
            showLogin = false;
          } else if (status === 'expired') {
            if (pollTimer) clearInterval(pollTimer);
            pollTimer = null;
          }
        } catch (cause) {
          error = cause instanceof Error ? cause.message : String(cause);
        }
      }, 2000);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  async function seek(ms: number) {
    try {
      await seekPlayer(Math.max(0, Math.round(ms)));
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  function togglePlay() {
    if (snapshot.state === 'playing') void pausePlayer();
    else void resumePlayer();
  }

  async function doLogout() {
    await logout();
    profile = null;
    playlists = [];
  }

  function closeLogin() {
    showLogin = false;
    if (pollTimer) clearInterval(pollTimer);
    pollTimer = null;
  }
</script>

<div class="titlebar" data-tauri-drag-region>
  <div class="window-controls">
    <button class="wc" aria-label="最小化" title="最小化" onclick={minimizeWindow}>
      <span class="i-min"></span>
    </button>
    <button class="wc" aria-label="最大化" title="最大化" onclick={toggleMaximize}>
      <span class="i-max"></span>
    </button>
    <button class="wc close" aria-label="关闭" title="关闭" onclick={closeWindow}>
      <span class="i-close"></span>
    </button>
  </div>
</div>

<div class="shell" class:with-lyrics={!!current}>
  <aside class="sidebar">
    <div class="side-search">
      <SearchBar bind:value={query} {loading} onsubmit={runSearch} />
    </div>

    <nav>
      {#each nav as item (item.key)}
        <button
          class:active={active === item.key}
          onclick={() => selectNav(item.key)}
          title={item.hint}
        >
          <Icon name={item.icon} size={16} />
          <span>{item.label}</span>
        </button>
      {/each}
    </nav>

    {#if playlists.length > 0}
      <div class="playlists">
        <p class="section">歌单</p>
        {#each playlists.slice(0, 12) as playlist (playlist.id)}
          <button
            class="playlist"
            onclick={() => openPlaylist(playlist)}
            onmouseenter={() => prefetchPlaylist(playlist.id)}
            title={playlist.name}
          >
            {playlist.name}
          </button>
        {/each}
      </div>
    {/if}

    <div class="side-foot">
      {#if settingsOpen}
        <div class="settings-pop" role="menu">
          {#if profile}
            <div class="pop-user">
              {#if profile.avatarUrl}
                <img class="pop-avatar" src={profile.avatarUrl} alt="" />
              {/if}
              <span class="pop-name" title={profile.nickname}>{profile.nickname}</span>
            </div>
            <div class="pop-sep"></div>
            <button class="pop-item" onclick={doLogout}>退出登录</button>
          {:else}
            <button class="pop-item" onclick={openLogin}>扫码登录</button>
            <div class="pop-sep"></div>
          {/if}
          <button
            class="pop-item"
            onclick={() => {
              settingsOpen = false;
              settingsPageOpen = true;
            }}>设置…</button
          >
          <button
            class="pop-item"
            onclick={() => {
              settingsOpen = false;
              showThemes = true;
            }}>主题外观</button
          >
        </div>
      {/if}
      <button
        class="settings-btn"
        class:active={settingsOpen}
        aria-label="设置"
        onclick={() => (settingsOpen = !settingsOpen)}
      >
        <Icon name="settings" size={16} />
        <span>设置</span>
      </button>
    </div>
  </aside>

  <main class="main">

    <section class="content">
      {#if active === 'local'}
        <div class="local-bar">
          <button class="local-action" onclick={pickLocalFolder}>添加文件夹</button>
          {#if localDirs.length > 0}
            <button class="local-action" onclick={rescanLocal} disabled={localScanning}>
              {localScanning ? '扫描中…' : '重新扫描'}
            </button>
            {#each localDirs as dir (dir)}
              <button class="local-chip" title={`移除 ${dir}`} onclick={() => dropLocalFolder(dir)}>
                {dir.split(/[\\/]/).filter(Boolean).pop() ?? dir}
                <span class="chip-x">×</span>
              </button>
            {/each}
          {/if}
        </div>
      {/if}
      {#if error}
        <div class="empty error">
          <p>出了点问题</p>
          <p class="dim">{error}</p>
        </div>
      {:else if !searched}
        <div class="empty">
          <h1>找点想听的</h1>
          <p class="dim">输入歌名或歌手，回车搜索；点任意一条会从这里开始排队播放。</p>
          {#if !profile}
            <p class="dim">登录后可以播放会员歌曲，并在左侧看到你的歌单。</p>
          {/if}
        </div>
      {:else if listLoading}
        <p class="result-count">{listTitle} · 加载中…</p>
        <div class="skeleton" aria-hidden="true">
          {#each Array(8) as _, index (index)}
            <div class="sk-row">
              <span class="sk-index"></span>
              <span class="sk-cover"></span>
              <span class="sk-lines">
                <span class="sk-line" style:width={`${58 + ((index * 13) % 30)}%`}></span>
                <span class="sk-line dim" style:width={`${34 + ((index * 7) % 22)}%`}></span>
              </span>
            </div>
          {/each}
        </div>
      {:else if tracks.length === 0}
        <div class="empty">
          {#if active === 'local'}
            <p>本地音乐还是空的</p>
            <p class="dim">点上面的「添加文件夹」选一个音乐目录，支持 mp3 / flac / m4a / wav 等。</p>
          {:else}
            <p>没找到「{query}」</p>
            <p class="dim">换个关键词试试</p>
          {/if}
        </div>
      {:else}
        <p class="result-count">{listTitle} · {tracks.length} 首</p>
        <TrackList
        {tracks}
        currentId={current?.id ?? null}
        playing={snapshot.state === 'playing'}
        onplay={play}
      />
      {/if}
    </section>
  </main>

  {#if current}
    <Lyrics {lyric} positionMs={snapshot.positionMs} onseek={seek} />
  {/if}

  <div class="player-slot">
    <PlayerBar
      track={current}
      {snapshot}
      {repeat}
      {shuffle}
      onpause={() => pausePlayer()}
      onresume={() => resumePlayer()}
      onnext={() => step('next')}
      onprevious={() => step('previous')}
      onmode={cyclePlayMode}
      
      onlyrics={flipLyrics}
      onopen={() => (stageOpen = true)}
      onseek={seek}
      onvolume={(volume) => setVolume(volume)}
      onqueue={() => (queueOpen = !queueOpen)}
      liked={current ? likedIds.has(current.id) : false}
      onlike={toggleLike}
    />
  </div>
</div>

<QueuePanel open={queueOpen} currentId={current?.id ?? null} onclose={() => (queueOpen = false)} />

<SettingsPanel
  open={settingsPageOpen}
  onclose={() => (settingsPageOpen = false)}
  ontheme={() => {
    settingsPageOpen = false;
    showThemes = true;
  }}
/>

{#if stageOpen && current}
  <NowPlaying
    track={current}
    {snapshot}
    {lyric}
    {repeat}
    {shuffle}
    onclose={() => (stageOpen = false)}
    onseek={seek}
    ontoggle={togglePlay}
    onnext={() => step('next')}
    onprevious={() => step('previous')}
    onmode={cyclePlayMode}
    
    onvolume={(volume) => setVolume(volume)}
  />
{/if}

{#if showThemes}
  <ThemePicker onclose={() => (showThemes = false)} />
{/if}

{#if showLogin}
  <div class="modal-backdrop" role="presentation" onclick={closeLogin}>
    <div class="modal" role="dialog" aria-modal="true" onclick={(event) => event.stopPropagation()}>
      <h2>扫码登录</h2>
      <p class="dim">打开网易云音乐 App，扫下方二维码确认</p>

      <div class="qr">
        {#if qrImage}
          <img src={qrImage} alt="登录二维码" />
        {:else}
          <div class="qr-placeholder">生成中…</div>
        {/if}
      </div>

      <p class="status">
        {#if qrStatus === 'scanned'}
          已扫描，请在手机上确认
        {:else if qrStatus === 'expired'}
          二维码已过期
        {:else if qrStatus === 'success'}
          登录成功
        {:else}
          等待扫码
        {/if}
      </p>

      <div class="modal-actions">
        <button class="ghost" onclick={refreshQr}>刷新二维码</button>
        <button class="ghost" onclick={closeLogin}>关闭</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .shell {
    display: grid;
    grid-template-columns: 220px 1fr;
    flex: 1;
    min-height: 0;
    overflow: hidden;
    position: relative;
    z-index: 1;
  }

  

  .shell.with-lyrics {
    grid-template-columns: 220px 1fr 300px;
  }

  .sidebar {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 18px 14px;
    background: var(--surface-1);
    border-right: 1px solid var(--border);
    min-height: 0;
  }





  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  nav button,
  .playlist {
    text-align: left;
    padding: 9px 12px;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: var(--text-dim);
    font-size: 13px;
    cursor: pointer;
    transition:
      background 120ms ease,
      color 120ms ease,
      transform 140ms ease;
  }

  .playlist:hover {
    transform: translateX(2px);
  }

  nav button:hover,
  .playlist:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  nav button.active {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    color: var(--accent);
  }

  .playlists {
    display: flex;
    flex-direction: column;
    gap: 2px;
    overflow-y: auto;
    min-height: 0;
  }

  .section {
    margin: 4px 0 2px;
    padding: 0 12px;
    font-size: 11px;
    color: var(--text-dim);
  }

  .playlist {
    padding: 6px 12px;
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }








  .main {
    display: flex;
    flex-direction: column;
    min-width: 0;
    overflow: hidden;
  }


  .content {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    /* 顶部留出 34px 标题栏浮层，否则前 34px 的点击会被浮层吃掉 */
    padding: 34px 24px 24px;
    display: flex;
    flex-direction: column;
  }

  .result-count {
    flex: none;
    font-size: 12px;
    color: var(--text-dim);
    margin: 4px 0 10px;
  }

  .empty {
    height: 100%;
    display: flex;
    flex-direction: column;
    justify-content: center;
    align-items: flex-start;
    gap: 8px;
    max-width: 520px;
  }

  .empty h1 {
    font-size: 26px;
    margin: 0;
    font-weight: 600;
  }

  .empty p {
    margin: 0;
  }

  .empty.error p:first-child {
    color: var(--danger);
  }

  .dim {
    color: var(--text-dim);
    font-size: 13px;
    line-height: 1.6;
  }

  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: grid;
    place-items: center;
    z-index: 20;
  }

  .modal {
    width: 320px;
    padding: 24px;
    border-radius: 16px;
    background: var(--surface-1);
    border: 1px solid var(--border);
    text-align: center;
  }

  .modal h2 {
    margin: 0 0 6px;
    font-size: 16px;
  }

  .qr {
    margin: 16px auto;
    width: 220px;
    height: 220px;
    background: #fff;
    border-radius: 10px;
    display: grid;
    place-items: center;
    overflow: hidden;
  }

  .qr img {
    width: 100%;
    height: 100%;
  }

  .qr-placeholder {
    color: #666;
    font-size: 12px;
  }

  .status {
    font-size: 12px;
    color: var(--text-dim);
    min-height: 18px;
  }

  .modal-actions {
    display: flex;
    gap: 8px;
    margin-top: 14px;
    justify-content: center;
  }

  .ghost {
    border: 1px solid var(--border);
    background: transparent;
    color: var(--text);
    border-radius: 8px;
    padding: 6px 14px;
    font-size: 12px;
    cursor: pointer;
  }

  .ghost:hover {
    background: var(--surface-2);
  }
  /* ===== Midnight Vinyl 覆盖层：侧栏去底色、导航编号、列表全宽 ===== */
  .shell {
    grid-template-columns: 248px 1fr;
  }

  .shell.with-lyrics {
    grid-template-columns: 248px 1fr 320px;
  }

  .sidebar {
    background: var(--surface-1);
    border-right: none;
    padding: 26px 20px 18px;
    gap: 0;
  }



  nav {
    gap: 0;
  }

  nav button {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 6px;
    border-radius: 0;
    background: transparent;
    color: var(--text-faint);
    font-size: 13px;
    transition: color 140ms ease;
  }

  nav button:hover {
    background: transparent;
    color: var(--text);
    transform: none;
  }

  nav button.active {
    background: transparent;
    color: var(--accent);
  }



  .section {
    text-transform: uppercase;
    letter-spacing: 0.16em;
    font-size: 10px;
    font-weight: 600;
    color: var(--text-faint);
    padding: 26px 6px 8px;
    margin: 0;
  }

  .playlist {
    padding: 5px 6px;
    border-radius: 0;
    font-size: 13px;
    color: var(--text-dim);
  }

  .playlist:hover {
    background: transparent;
    color: var(--text);
    transform: none;
  }


  .content {
    padding: 0;
  }

  .result-count {
    padding: 14px 32px 12px;
    margin: 0;
    font-family: var(--font-mono);
    font-size: 11px;
    letter-spacing: 0.02em;
    color: var(--text-faint);
  }

  .empty h1 {
    font-family: var(--font-ui);
    font-weight: 600;
    font-size: 30px;
    line-height: 1.15;
    letter-spacing: -0.005em;
    margin: 0 0 10px;
  }

  .empty p {
    font-size: 13px;
  }

  .empty p.dim {
    color: var(--text-faint);
  }

  .empty {
    padding: 56px 32px;
    height: auto;
  }








  .titlebar {
    position: absolute;
    top: 0;
    left: var(--sidebar-w, 248px);
    right: 0;
    height: 34px;
    z-index: 40;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    padding: 0 4px;
    background: transparent;
    user-select: none;
  }

  .window-controls {
    margin-left: auto;
    display: flex;
    align-items: stretch;
  }

  .wc {
    width: 44px;
    height: 34px;
    border: none;
    background: transparent;
    color: var(--text-faint);
    display: grid;
    place-items: center;
    cursor: pointer;
    transition:
      background 120ms ease,
      color 120ms ease;
  }

  .wc:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .wc.close:hover {
    background: #d9534a;
    color: #fff;
  }

  .i-min {
    width: 11px;
    height: 1px;
    background: currentColor;
  }

  .i-max {
    width: 10px;
    height: 10px;
    border: 1px solid currentColor;
    border-radius: 1px;
  }

  .i-close {
    position: relative;
    width: 11px;
    height: 11px;
  }

  .i-close::before,
  .i-close::after {
    content: '';
    position: absolute;
    left: 0;
    top: 5px;
    width: 11px;
    height: 1px;
    background: currentColor;
    transform: rotate(45deg);
  }

  .i-close::after {
    transform: rotate(-45deg);
  }

  /* ===== 侧栏：图标导航 + 顶部搜索 ===== */
  .sidebar {
    background: var(--surface-1);
    border-right: none;
    padding: 8px 12px 12px;
    gap: 0;
  }

  .side-search {
    padding: 2px 4px 16px;
  }

  nav {
    gap: 2px;
  }

  nav button {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border-radius: 7px;
    background: transparent;
    color: var(--text-dim);
    font-size: 13px;
    transition:
      background 140ms ease,
      color 140ms ease;
  }

  nav button:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  nav button.active {
    background: var(--surface-2);
    color: var(--text);
  }

  nav button.active :global(svg) {
    color: var(--accent);
  }

  .main {
    background: transparent;
  }

  .content {
    padding: 34px 0 0;
  }  /* ===== 播放条从侧栏右侧开始：侧栏通到底 ===== */
  .shell {
    grid-template-columns: var(--sidebar-w, 248px) 1fr;
    grid-template-rows: minmax(0, 1fr) auto;
  }

  .shell.with-lyrics {
    grid-template-columns: var(--sidebar-w, 248px) 1fr 320px;
  }

  .sidebar {
    grid-column: 1;
    grid-row: 1 / -1;
  }

  .main {
    grid-column: 2;
    grid-row: 1;
    min-height: 0;
  }

  .player-slot {
    grid-column: 2 / -1;
    grid-row: 2;
    min-width: 0;
    min-height: 0;
  }  /* ===== 侧栏底部：设置入口 + 弹出菜单 ===== */
  .side-foot {
    margin-top: auto;
    position: relative;
    padding-top: 10px;
  }

  .settings-btn {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 10px;
    border: none;
    border-radius: 7px;
    background: transparent;
    color: var(--text-dim);
    font-size: 13px;
    cursor: pointer;
    transition:
      background 140ms ease,
      color 140ms ease;
  }

  .settings-btn:hover,
  .settings-btn.active {
    background: var(--surface-2);
    color: var(--text);
  }

  .settings-pop {
    position: absolute;
    left: 0;
    right: 0;
    bottom: calc(100% + 8px);
    z-index: 30;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 6px;
    border-radius: 10px;
    background: var(--surface-2);
    box-shadow: var(--shadow-2);
    animation: pop-in 140ms ease;
  }

  @keyframes pop-in {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .pop-user {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 10px 6px;
    min-width: 0;
  }

  .pop-avatar {
    width: 22px;
    height: 22px;
    border-radius: 5px;
    object-fit: cover;
    background: var(--surface-3);
  }

  .pop-name {
    font-size: 12px;
    color: var(--text-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .pop-sep {
    height: 1px;
    margin: 4px 6px;
    background: var(--border);
  }

  .pop-item {
    text-align: left;
    padding: 8px 10px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text-dim);
    font-size: 13px;
    cursor: pointer;
    transition:
      background 120ms ease,
      color 120ms ease;
  }

  .pop-item:hover {
    background: color-mix(in srgb, var(--text) 7%, transparent);
    color: var(--text);
  }  /* ===== 歌单加载骨架：立刻给出反馈，避免"点了没反应" ===== */
  .skeleton {
    padding: 6px 28px;
    animation: sk-pulse 1.3s ease-in-out infinite;
  }

  .sk-row {
    display: grid;
    grid-template-columns: 40px 40px 1fr;
    align-items: center;
    gap: 16px;
    height: 60px;
  }

  .sk-index {
    width: 12px;
    height: 10px;
    border-radius: 3px;
    background: var(--border-strong);
  }

  .sk-cover {
    width: 40px;
    height: 40px;
    border-radius: 5px;
    background: var(--surface-2);
  }

  .sk-lines {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  .sk-line {
    height: 9px;
    border-radius: 4px;
    background: var(--surface-2);
  }

  .sk-line.dim {
    height: 7px;
    background: color-mix(in srgb, var(--surface-2) 65%, transparent);
  }

  @keyframes sk-pulse {
    0%,
    100% {
      opacity: 0.5;
    }
    50% {
      opacity: 1;
    }
  }  /* 本地音乐工具条 */
  .local-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin: 0 0 4px;
    padding: 0 0 8px;
  }

  .local-action {
    height: 30px;
    padding: 0 14px;
    border: none;
    border-radius: 999px;
    background: color-mix(in srgb, var(--text) 8%, transparent);
    color: var(--text);
    font-size: 12.5px;
    font-weight: 500;
    cursor: pointer;
  }

  .local-action:hover {
    background: color-mix(in srgb, var(--text) 13%, transparent);
  }

  .local-action:disabled {
    opacity: 0.55;
    cursor: default;
  }

  .local-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 8px 0 10px;
    border: none;
    border-radius: 999px;
    background: transparent;
    color: var(--text-dim);
    font-size: 12px;
    cursor: pointer;
    max-width: 200px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .local-chip:hover {
    background: color-mix(in srgb, var(--text) 7%, transparent);
    color: var(--text);
  }

  .chip-x {
    font-size: 13px;
    opacity: 0.6;
  }</style>
























































