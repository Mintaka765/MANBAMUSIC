<script lang="ts">
  import Icon from './Icon.svelte';
  import { coverUrl } from './api';
  import type { PlayerSnapshot, Track } from './types';

  let {
    track,
    snapshot,
    repeat,
    shuffle,
    onpause,
    onresume,
    onnext,
    onprevious,
    onrepeat,
    onshuffle,
    onlyrics,
    onopen,
    onseek,
    onvolume,
    onqueue,
    liked = false,
    onlike,
  }: {
    track: Track | null;
    snapshot: PlayerSnapshot;
    repeat: 'off' | 'all' | 'one';
    shuffle: boolean;
    onpause: () => void;
    onresume: () => void;
    onnext: () => void;
    onprevious: () => void;
    onrepeat: () => void;
    onshuffle: () => void;
    onlyrics: () => void;
    onopen: () => void;
    onseek: (ms: number) => void;
    onvolume: (volume: number) => void;
    onqueue?: () => void;
    liked?: boolean;
    onlike?: () => void;
  } = $props();

  let volumeDragging = $state(false);
  /** 拖动中的本地预览值：不等后端 400ms 轮询，跟手 */
  let volumePreview = $state<number | null>(null);
  let seeking = $state(false);
  let seekPreview = $state<number | null>(null);
  let lastVolumeSent = 0;
  let muted = $state(false);
  let lastVolume = 0.8;

  function volumeAt(clientX: number, element: HTMLElement) {
    const rect = element.getBoundingClientRect();
    const ratio = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
    const next = Math.round(ratio * 100) / 100;
    if (next > 0) {
      muted = false;
      lastVolume = next;
    }
    volumePreview = next;
    const now = Date.now();
    if (now - lastVolumeSent > 60) {
      lastVolumeSent = now;
      onvolume(next);
    }
  }

  function onVolumeDown(event: PointerEvent) {
    const element = event.currentTarget as HTMLElement;
    try {
      element.setPointerCapture(event.pointerId);
    } catch {
      // 合成事件没有真实 pointerId，忽略即可
    }
    volumeDragging = true;
    volumeAt(event.clientX, element);
  }

  function onVolumeMove(event: PointerEvent) {
    if (!volumeDragging) return;
    volumeAt(event.clientX, event.currentTarget as HTMLElement);
  }

  function onVolumeUp(event: PointerEvent) {
    volumeDragging = false;
    const element = event.currentTarget as HTMLElement;
    try {
      if (element.hasPointerCapture(event.pointerId)) {
        element.releasePointerCapture(event.pointerId);
      }
    } catch {
      // 同上
    }
    if (volumePreview != null) onvolume(volumePreview);
    setTimeout(() => (volumePreview = null), 350);
  }

  function nudge(delta: number) {
    const next = Math.max(0, Math.min(1, Math.round((snapshot.volume + delta) * 100) / 100));
    if (next > 0) {
      muted = false;
      lastVolume = next;
    }
    onvolume(next);
  }

  function onVolumeKey(event: KeyboardEvent) {
    if (event.key === 'ArrowRight' || event.key === 'ArrowUp') nudge(0.05);
    else if (event.key === 'ArrowLeft' || event.key === 'ArrowDown') nudge(-0.05);
    else if (event.key === 'Home') onvolume(0);
    else if (event.key === 'End') onvolume(1);
    else return;
    event.preventDefault();
  }

  function onVolumeWheel(event: WheelEvent) {
    event.preventDefault();
    nudge(event.deltaY < 0 ? 0.05 : -0.05);
  }

  function toggleMute() {
    if (snapshot.volume > 0) {
      lastVolume = snapshot.volume;
      muted = true;
      onvolume(0);
    } else {
      muted = false;
      onvolume(lastVolume || 0.8);
    }
  }

  function positionAt(clientX: number, element: HTMLElement): number {
    const rect = element.getBoundingClientRect();
    const ratio = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
    return Math.round(ratio * snapshot.durationMs);
  }

  function onSeekDown(event: PointerEvent) {
    if (!snapshot.durationMs) return;
    const element = event.currentTarget as HTMLElement;
    try {
      element.setPointerCapture(event.pointerId);
    } catch {
      // 合成事件忽略
    }
    seeking = true;
    seekPreview = positionAt(event.clientX, element);
  }

  function onSeekMove(event: PointerEvent) {
    if (!seeking) return;
    seekPreview = positionAt(event.clientX, event.currentTarget as HTMLElement);
  }

  function onSeekUp(event: PointerEvent) {
    if (!seeking) return;
    seeking = false;
    const element = event.currentTarget as HTMLElement;
    try {
      if (element.hasPointerCapture(event.pointerId)) {
        element.releasePointerCapture(event.pointerId);
      }
    } catch {
      // 同上
    }
    if (seekPreview != null) onseek(seekPreview);
    // 兜底：万一后端一直没回来，最多显示 8 秒预览
    setTimeout(() => (seekPreview = null), 8000);
  }

  function seekKey(event: KeyboardEvent) {
    if (!snapshot.durationMs) return;
    const step = 5000;
    if (event.key === 'ArrowRight') onseek(Math.min(snapshot.durationMs, snapshot.positionMs + step));
    else if (event.key === 'ArrowLeft') onseek(Math.max(0, snapshot.positionMs - step));
    else return;
    event.preventDefault();
  }


  /** 拖动中用预览值，静止时用后端快照 */
  const shownPosition = $derived(seekPreview ?? snapshot.positionMs);
  const shownVolume = $derived(volumePreview ?? snapshot.volume);
  const progress = $derived(
    snapshot.durationMs > 0 ? Math.min(shownPosition / snapshot.durationMs, 1) : 0,
  );
  // 快照追上目标位置就撤掉预览：反向 seek 可能要 1~3 秒，靠固定超时会闪回
  $effect(() => {
    if (seeking) return;
    const target = seekPreview;
    if (target == null) return;
    if (Math.abs(snapshot.positionMs - target) < 1500) seekPreview = null;
  });

  const playing = $derived(snapshot.state === 'playing');
  const loading = $derived(snapshot.state === 'loading');

  function format(ms: number): string {
    if (!ms) return '--:--';
    const total = Math.floor(ms / 1000);
    return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, '0')}`;
  }
</script>

<footer class="player">
  <div class="now-wrap">
    <button class="now" onclick={onopen} title="打开正在播放">
    {#if track}
      {#if coverUrl(track.coverUrl, 80)}
        <img class="cover" src={coverUrl(track.coverUrl, 80)} alt="" />
      {:else}
        <div class="cover placeholder" aria-hidden="true">{track.name.slice(0, 1)}</div>
      {/if}
      <div class="info">
        <span class="title">
          {track.name}
          {#if snapshot.trialSecs}
            <span class="trial">试听 {snapshot.trialSecs}s</span>
          {/if}
        </span>
        <span class="meta">{track.artists.join(' / ')}</span>
      </div>
    {:else}
      <div class="cover empty" aria-hidden="true"></div>
      <div class="info">
        <span class="title muted">未在播放</span>
        <span class="meta">从搜索结果里选一首</span>
      </div>
    {/if}
    </button>
    {#if track}
      <button
        class="like"
        class:on={liked}
        aria-label={liked ? '取消喜欢' : '喜欢'}
        title={liked ? '取消喜欢' : '喜欢'}
        onclick={onlike}
      >
        <Icon name="heart" size={17} />
      </button>
    {/if}
  </div>

  <div class="controls">
    <div class="transport">
    <button class="icon" aria-label="上一首" onclick={onprevious}><Icon name="prev" size={16} /></button>
    <button
      class="play"
      aria-label={playing ? '暂停' : '播放'}
      disabled={loading || !track}
      onclick={() => (playing ? onpause() : onresume())}
    >
      {#if loading}<span class="dots">…</span>{:else}<Icon name={playing ? 'pause' : 'play'} size={18} />{/if}
    </button>
    <button class="icon" aria-label="下一首" onclick={onnext}><Icon name="next" size={16} /></button>

    <button
      class="icon mode"
      class:on={repeat !== 'off'}
      aria-label="循环模式"
      title={repeat === 'one' ? '单曲循环' : repeat === 'all' ? '列表循环' : '不循环'}
      onclick={onrepeat}
    >
      <Icon name={repeat === 'one' ? 'repeatOne' : 'repeat'} size={16} />
    </button>
    <button
      class="icon mode"
      class:on={shuffle}
      aria-label="随机播放"
      title={shuffle ? '随机开启' : '随机关闭'}
      onclick={onshuffle}
    >
      <Icon name="shuffle" size={16} />
    </button>

    <button class="icon mode" aria-label="桌面歌词" title="桌面歌词" onclick={onlyrics}>
      <span class="char" aria-hidden="true">词</span>
    </button>

    <button class="icon mode" aria-label="待播清单" title="待播清单" onclick={onqueue}>
      <Icon name="queue" size={17} />
    </button>

    </div>
    <div class="seek">
    <div
      class="progress"
      class:seeking
      role="slider"
      tabindex="0"
      aria-label="播放进度"
      aria-valuemin="0"
      aria-valuemax={snapshot.durationMs}
      aria-valuenow={shownPosition}
      onpointerdown={onSeekDown}
      onpointermove={onSeekMove}
      onpointerup={onSeekUp}
      onpointercancel={onSeekUp}
      onkeydown={seekKey}
    >
      <div class="bar"><div class="fill" style:width={`${progress * 100}%`}></div></div>
    </div>
    <span class="time">
      {format(shownPosition)} / {format(snapshot.durationMs || track?.durationMs || 0)}
    </span>
    </div>
  </div>

  <div class="volume">
    {#if snapshot.error}
      <span class="error" title={snapshot.error}>{snapshot.error}</span>
    {:else}
      <button
        class="vol-btn"
        aria-label={muted ? '取消静音' : '静音'}
        title={muted ? '取消静音' : '静音'}
        onclick={toggleMute}
      >
        <Icon name="volume" size={15} />
      </button>
      <div
        class="volume-track"
        role="slider"
        tabindex="0"
        aria-label="音量"
        aria-valuemin="0"
        aria-valuemax="100"
        aria-valuenow={Math.round(snapshot.volume * 100)}
        onpointerdown={onVolumeDown}
        onpointermove={onVolumeMove}
        onpointerup={onVolumeUp}
        onpointercancel={onVolumeUp}
        onkeydown={onVolumeKey}
        onwheel={onVolumeWheel}
      >
        <div class="volume-rail">
          <div class="volume-fill" style:width={`${shownVolume * 100}%`}></div>
          <div class="volume-thumb" style:left={`${shownVolume * 100}%`}></div>
        </div>
      </div>
    {/if}
  </div>
</footer>

<style>
  .player {
    position: relative;
    z-index: 1;
    display: grid;
    grid-template-columns: minmax(220px, 1fr) minmax(360px, 1.4fr) minmax(160px, 1fr);
    align-items: center;
    gap: 28px;
    height: 88px;
    padding: 0 24px;
    background: var(--surface-1);
    border-top: 1px solid var(--border);
  }

  .now {
    display: flex;
    align-items: center;
    gap: 14px;
    min-width: 0;
    border: none;
    background: transparent;
    color: inherit;
    padding: 0;
    cursor: pointer;
    text-align: left;
  }

  .now:hover .title {
    color: var(--accent);
  }

  .cover {
    width: 56px;
    height: 56px;
    flex: none;
    border-radius: 3px;
    object-fit: cover;
    background: var(--surface-2);
    box-shadow: var(--shadow-1);
    transition: transform 200ms cubic-bezier(0.2, 0.7, 0.3, 1);
  }

  .now:hover .cover {
    transform: scale(1.04);
  }

  .cover.placeholder,
  .cover.empty {
    display: grid;
    place-items: center;
    background: var(--surface-2);
    color: var(--text-faint);
    font-family: var(--font-display);
    font-weight: 400;
    font-size: 20px;
  }

  .info {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }

  .title {
    font-family: var(--font-ui);
    font-size: 14px;
    font-weight: 500;
    line-height: 1.3;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    transition: color 140ms ease;
  }

  .title.muted {
    color: var(--text-dim);
  }

  .trial {
    margin-left: 8px;
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.06em;
    color: var(--warning);
    vertical-align: middle;
  }

  .meta {
    font-size: 12px;
    color: var(--text-faint);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .controls {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .transport {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .seek {
    display: flex;
    align-items: center;
    gap: 14px;
    width: 100%;
    max-width: 420px;
  }

  .icon,
  .play {
    border: none;
    background: transparent;
    color: var(--text);
    cursor: pointer;
    transition:
      color 140ms ease,
      transform 120ms ease,
      background 140ms ease;
  }

  .icon {
    width: 30px;
    height: 30px;
    display: grid;
    place-items: center;
    border-radius: 0;
    color: var(--text-faint);
  }

  .icon:hover:not(:disabled) {
    color: var(--text);
  }

  .dots {
    font-family: var(--font-mono);
    font-size: 14px;
    line-height: 1;
  }

  .char {
    font-family: var(--font-ui);
    font-size: 12px;
    font-weight: 600;
    line-height: 1;
  }

  .icon:active:not(:disabled),
  .play:active:not(:disabled) {
    transform: scale(0.92);
  }

  .icon:disabled,
  .play:disabled {
    opacity: 0.3;
    cursor: default;
  }

  .mode {
    color: var(--text-faint);
    transition: color 120ms ease;
  }

  .mode.on {
    color: var(--accent);
  }

  /* 播放键只保留图形本身，不加圆形底 */
  .play {
    width: 36px;
    height: 36px;
    margin: 0 4px;
    border-radius: 6px;
    background: transparent;
    color: var(--accent);
    display: grid;
    place-items: center;
    transition: color 140ms ease;
  }

  .play:hover:not(:disabled) {
    background: transparent;
    color: color-mix(in srgb, var(--accent) 82%, var(--text));
  }

  .progress {
    flex: 1;
    display: flex;
    align-items: center;
    cursor: pointer;
  }

  .progress:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 6px;
    border-radius: 2px;
  }

  .bar {
    width: 100%;
    height: 2px;
    border-radius: 999px;
    background: var(--border-strong);
    overflow: hidden;
  }

  .fill {
    height: 100%;
    background: var(--accent);
    border-radius: 999px;
    /* 后端 400ms 推一次进度，线性过渡正好把跳变抹平成连续运动 */
    transition: width 420ms linear;
  }

  .time {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-faint);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    letter-spacing: 0.02em;
  }

  .volume {
    display: flex;
    align-items: center;
    gap: 10px;
    justify-content: flex-end;
    color: var(--text-faint);
  }



  .error {
    font-size: 11px;
    color: var(--danger);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* ===== 音量控件（可点、可拖、可滚轮、可静音）===== */
  .volume {
    gap: 12px;
  }

  .vol-btn {
    border: none;
    background: transparent;
    color: var(--text-faint);
    width: 26px;
    height: 26px;
    display: grid;
    place-items: center;
    cursor: pointer;
    transition: color 140ms ease;
  }

  .vol-btn:hover {
    color: var(--text);
  }

  .volume-track {
    width: 72px;
    height: 22px;
    display: flex;
    align-items: center;
    cursor: pointer;
    touch-action: none;
  }

  .volume-rail {
    position: relative;
    width: 100%;
    height: 3px;
    border-radius: 999px;
    background: var(--border-strong);
  }

  .volume-fill {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    background: var(--accent);
    opacity: 0.55;
    border-radius: 999px;
    transition: width 120ms linear;
  }

  .volume-thumb {
    position: absolute;
    top: 50%;
    width: 9px;
    height: 9px;
    margin-left: -4.5px;
    margin-top: -4.5px;
    border-radius: 50%;
    background: var(--accent);
    opacity: 0;
    transition: opacity 140ms ease;
  }

  .volume-track:hover .volume-thumb,
  .volume-track:focus-visible .volume-thumb,
  .volume-track:active .volume-thumb {
    opacity: 1;
  }

  .volume-track:focus-visible {
    outline: none;
  }

  .volume-track:focus-visible .volume-rail {
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 40%, transparent);
  }

  /* ===== 密度：播放条矮一点，更像系统原生控件 ===== */
  .player {
    height: var(--player-h);
    gap: 24px;
    padding: 0 22px;
  }

  .cover {
    width: 52px;
    height: 52px;
    border-radius: 8px;
  }  .now-wrap {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .now-wrap .now {
    min-width: 0;
  }

  .like {
    flex: none;
    width: 32px;
    height: 32px;
    display: grid;
    place-items: center;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: var(--text-faint);
    cursor: pointer;
    transition:
      color 140ms ease,
      background 140ms ease;
  }

  .like:hover {
    color: var(--text);
  }

  .like.on {
    color: var(--accent);
  }

  .like.on :global(svg) {
    fill: currentColor;
  }  /* 拖动时去掉缓动，否则指针跟手感被 420ms 过渡拖慢 */
  .progress.seeking .fill {
    transition: none;
  }

  .progress.seeking {
    cursor: grabbing;
  }

  .volume-track {
    cursor: pointer;
  }

  .volume-track:active {
    cursor: grabbing;
  }</style>


















