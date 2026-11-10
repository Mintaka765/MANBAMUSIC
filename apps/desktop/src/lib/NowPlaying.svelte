<script lang="ts">
  import Icon from './Icon.svelte';
  import Lyrics from './Lyrics.svelte';
  import { coverUrl } from './api';
  import type { LyricTrack, PlayerSnapshot, Track } from './types';

  let {
    track,
    snapshot,
    lyric,
    repeat,
    shuffle,
    onclose,
    onseek,
    ontoggle,
    onnext,
    onprevious,
    onmode,
    onvolume,
  }: {
    track: Track;
    snapshot: PlayerSnapshot;
    lyric: LyricTrack | null;
    repeat: 'off' | 'all' | 'one';
    shuffle: boolean;
    onclose: () => void;
    onseek: (ms: number) => void;
    ontoggle: () => void;
    onnext: () => void;
    onprevious: () => void;
    onmode: () => void;
    onvolume: (volume: number) => void;
  } = $props();

  let volumeDragging = $state(false);
  /** 拖动中的本地预览值：不等 400ms 轮询，跟手 */
  let volumePreview = $state<number | null>(null);
  let seeking = $state(false);
  let seekPreview = $state<number | null>(null);
  let lastVolumeSent = 0;

  function volumeAt(clientX: number, element: HTMLElement) {
    const rect = element.getBoundingClientRect();
    const ratio = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
    const next = Math.round(ratio * 100) / 100;
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
      // 合成事件没有真实 pointerId
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

  function onVolumeWheel(event: WheelEvent) {
    event.preventDefault();
    const next = Math.max(0, Math.min(1, Math.round((snapshot.volume + (event.deltaY < 0 ? 0.05 : -0.05)) * 100) / 100));
    onvolume(next);
  }

  // 快照追上目标位置就撤掉预览：反向 seek 可能要 1~3 秒，靠固定超时会闪回
  $effect(() => {
    if (seeking) return;
    const target = seekPreview;
    if (target == null) return;
    if (Math.abs(snapshot.positionMs - target) < 1500) seekPreview = null;
  });

  const playing = $derived(snapshot.state === 'playing');
  const shownPosition = $derived(seekPreview ?? snapshot.positionMs);
  const shownVolume = $derived(volumePreview ?? snapshot.volume);
  const progress = $derived(
    snapshot.durationMs > 0 ? Math.min(shownPosition / snapshot.durationMs, 1) : 0,
  );

  function format(ms: number): string {
    if (!ms) return '--:--';
    const total = Math.floor(ms / 1000);
    return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, '0')}`;
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
</script>

<section class="stage">
  <header>
    <button class="close" aria-label="收起" onclick={onclose}>⏷</button>
    <span class="label">正在播放</span>
    <span class="spacer"></span>
  </header>

  <div class="body">
    <div class="left">
      {#if coverUrl(track.coverUrl, 600)}
        <img
          class="cover"
          src={coverUrl(track.coverUrl, 600)}
          alt={track.album}
          loading="lazy"
          decoding="async"
        />
      {:else}
        <div class="cover placeholder">{track.name.slice(0, 1)}</div>
      {/if}

      <h1 class="title">{track.name}</h1>
      <p class="meta">
        {track.artists.join(' / ')}
        {#if track.album}· {track.album}{/if}
      </p>

      <div
        class="bar"
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
        <div class="fill" style:width={`${progress * 100}%`}></div>
      </div>
      <div class="times">
        <span>{format(shownPosition)}</span>
        <span>{format(snapshot.durationMs || track.durationMs)}</span>
      </div>

      <div class="controls">
        <button class="icon" aria-label="上一首" onclick={onprevious}>
          <Icon name="prev" size={20} />
        </button>
        <button class="play" aria-label={playing ? '暂停' : '播放'} onclick={ontoggle}>
          {#if snapshot.state === 'loading'}
            <span class="dots">…</span>
          {:else}
            <Icon name={playing ? 'pause' : 'play'} size={22} />
          {/if}
        </button>
        <button class="icon" aria-label="下一首" onclick={onnext}>
          <Icon name="next" size={20} />
        </button>
        <button
          class="icon"
          class:on={shuffle || repeat !== 'off'}
          aria-label={shuffle ? '随机播放' : repeat === 'one' ? '单曲循环' : repeat === 'all' ? '列表循环' : '顺序播放'}
          onclick={onmode}
        >
          <Icon name={shuffle ? 'shuffle' : repeat === 'one' ? 'repeatOne' : 'repeat'} size={18} />
        </button>
      </div>

      <div class="volume-row">
        <Icon name="volume" size={15} />
        <div
          class="vol-track"
          role="slider"
          tabindex="0"
          aria-label="音量"
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow={Math.round(shownVolume * 100)}
          onpointerdown={onVolumeDown}
          onpointermove={onVolumeMove}
          onpointerup={onVolumeUp}
          onpointercancel={onVolumeUp}
          onwheel={onVolumeWheel}
        >
          <div class="vol-rail">
            <div class="vol-fill" style:width={`${shownVolume * 100}%`}></div>
            <div class="vol-thumb" style:left={`${shownVolume * 100}%`}></div>
          </div>
        </div>
      </div>

      {#if snapshot.trialSecs}
        <p class="trial">试听 {snapshot.trialSecs} 秒</p>
      {/if}
      {#if snapshot.error}
        <p class="error">{snapshot.error}</p>
      {/if}
    </div>

    <div class="right">
      <Lyrics {lyric} positionMs={snapshot.positionMs} variant="stage" onseek={onseek} />
    </div>
  </div>
</section>

<style>
  .stage {
    position: fixed;
    inset: 0;
    z-index: 10;
    display: flex;
    flex-direction: column;
    /* 不透明底色：背景里的队列不再透出来，歌词对比度更干净。
       顶部叠一层极淡的强调色光，避免整页死板。 */
    background-color: var(--bg);
    background-image:
      radial-gradient(1200px 680px at 14% -12%, color-mix(in srgb, var(--accent) 7%, transparent), transparent 64%),
      radial-gradient(900px 520px at 104% 4%, color-mix(in srgb, var(--accent) 4%, transparent), transparent 58%);
    background-attachment: fixed;
    animation: rise 220ms ease;
  }

  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(12px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 40px 20px 4px;
    color: var(--text-dim);
    font-size: 12px;
  }

  .close {
    width: 32px;
    height: 32px;
    display: grid;
    place-items: center;
    border: none;
    background: transparent;
    color: var(--text-dim);
    font-size: 16px;
    cursor: pointer;
    border-radius: 50%;
    transition:
      background 140ms ease,
      color 140ms ease;
  }

  .close:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .label {
    letter-spacing: 1px;
  }

  .spacer {
    flex: 1;
  }

  .body {
    flex: 1;
    min-height: 0;
    display: grid;
    /* 左边留白比右边大：封面块整体右移，和右侧居中的歌词凑成一个视觉组 */
    grid-template-columns: 300px minmax(0, 1fr);
    gap: 64px;
    width: 100%;
    max-width: 1280px;
    margin: 0 auto;
    padding: 12px 40px 32px 12%;
  }

  @media (max-width: 1100px) {
    .body {
      grid-template-columns: 260px minmax(0, 1fr);
      gap: 32px;
      padding-left: 40px;
    }

    .left .cover {
      width: 230px;
      height: 230px;
    }
  }

  .left {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    min-width: 0;
  }

  .cover {
    width: 300px;
    height: 300px;
    border-radius: var(--radius-lg);
    object-fit: cover;
    box-shadow: var(--shadow-2);
    background: var(--surface-2);

  }

  @keyframes cover-in {
    from {
      opacity: 0;
      transform: scale(0.96) translateY(8px);
    }
    to {
      opacity: 1;
      transform: scale(1) translateY(0);
    }
  }

  .cover.placeholder {
    display: grid;
    place-items: center;
    font-size: 64px;
    color: var(--text-dim);
    background: linear-gradient(
      135deg,
      var(--accent),
      color-mix(in srgb, var(--accent) 35%, var(--bg))
    );
  }

  .title {
    margin: 10px 0 0;
    font-size: 22px;
    font-weight: 700;
    text-align: center;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta {
    margin: 0;
    font-size: 12px;
    color: var(--text-dim);
    text-align: center;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .bar {
    width: 100%;
    max-width: 360px;
    height: 5px;
    margin-top: 12px;
    border-radius: 999px;
    background: var(--surface-2);
    overflow: hidden;
    cursor: pointer;
  }

  .bar:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 3px;
  }

  .fill {
    height: 100%;
    background: var(--accent);
    border-radius: 999px;
    box-shadow: 0 0 10px color-mix(in srgb, var(--accent) 60%, transparent);
    transition: width 420ms linear;
  }

  .times {
    width: 100%;
    max-width: 360px;
    display: flex;
    justify-content: space-between;
    font-size: 11px;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }

  .controls {
    display: flex;
    align-items: center;
    gap: 18px;
    margin-top: 8px;
  }

  .icon,
  .play {
    border: none;
    background: transparent;
    color: var(--text);
    cursor: pointer;
    font-size: 16px;
  }

  .icon {
    width: 40px;
    height: 40px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    color: var(--text-dim);
    transition:
      color 140ms ease,
      background 140ms ease,
      transform 120ms ease;
  }

  .icon:hover {
    color: var(--text);
    background: var(--surface-2);
  }

  .icon:active {
    transform: scale(0.92);
  }

  .dots {
    font-size: 16px;
    line-height: 1;
  }

  .play {
    width: 48px;
    height: 48px;
    border-radius: 50%;
    background: var(--accent);
    color: var(--accent-contrast);
    font-size: 18px;
    display: grid;
    place-items: center;
    transition:
      transform 120ms ease,
      filter 140ms ease;
  }

  .play:active {
    transform: scale(0.92);
  }

  .play:hover {
    filter: brightness(1.08);
  }

  .trial {
    margin: 6px 0 0;
    font-size: 11px;
    color: var(--warning);
  }

  .error {
    margin: 6px 0 0;
    font-size: 11px;
    color: var(--danger);
    max-width: 360px;
    text-align: center;
  }

  .right {
    min-width: 0;
    min-height: 0;
    display: flex;
  }

  .right :global(.lyrics) {
    width: 100%;
  }
  /* ===== Midnight Vinyl ===== */
  .body {
    grid-template-columns: 320px minmax(0, 1fr);
    gap: 72px;
    max-width: 1240px;
    padding: 12px 40px 40px 10%;
  }

  .cover {
    width: 320px;
    height: 320px;
    border-radius: 14px;
    box-shadow: var(--shadow-2);
  }

  .title {
    font-family: var(--font-ui);
    font-weight: 600;
    font-size: 28px;
    line-height: 1.2;
    letter-spacing: -0.01em;
    margin-top: 22px;
  }

  .meta {
    font-size: 13px;
    color: var(--text-faint);
  }

  .times span {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-faint);
  }

  .bar {
    height: 2px;
    background: var(--border-strong);
    margin-top: 18px;
  }

  .fill {
    background: var(--accent);
  }

  .play {
    background: var(--accent);
    color: var(--accent-contrast);
  }

  .icon {
    color: var(--text-faint);
  }  /* ===== 控制区：随机 / 上一首 / 播放 / 下一首 / 循环 + 音量 ===== */
  .controls {
    gap: 18px;
    margin-top: 6px;
  }

  .icon {
    width: 40px;
    height: 40px;
    color: var(--text-faint);
  }

  .icon:hover {
    color: var(--text);
    background: transparent;
  }

  .icon.on {
    color: var(--accent);
  }

  .play {
    width: 44px;
    height: 44px;
    margin: 0 6px;
    background: transparent;
    color: var(--accent);
  }

  .play:hover:not(:disabled) {
    background: transparent;
    color: color-mix(in srgb, var(--accent) 82%, var(--text));
  }

  .volume-row {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    max-width: 168px;
    margin-top: 10px;
    color: var(--text-faint);
  }

  .vol-track {
    flex: 1;
    height: 20px;
    display: flex;
    align-items: center;
    cursor: pointer;
    touch-action: none;
  }

  .vol-rail {
    position: relative;
    width: 100%;
    height: 3px;
    border-radius: 999px;
    background: var(--border-strong);
  }

  .vol-fill {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    background: var(--accent);
    opacity: 0.55;
    border-radius: 999px;
    transition: width 120ms linear;
  }

  .vol-thumb {
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

  .vol-track:hover .vol-thumb,
  .vol-track:active .vol-thumb,
  .vol-track:focus-visible .vol-thumb {
    opacity: 1;
  }

  .vol-track:focus-visible {
    outline: none;
  }

  .vol-track:focus-visible .vol-rail {
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 40%, transparent);
  }




  .bar.seeking .fill {
    transition: none;
  }

  .bar.seeking {
    cursor: grabbing;
  }</style>























