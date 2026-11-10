<script lang="ts">
  import type { LyricTrack } from './types';

  let {
    lyric,
    positionMs,
    variant = 'panel',
    onseek,
  }: {
    lyric: LyricTrack | null;
    positionMs: number;
    variant?: 'panel' | 'stage';
    onseek?: (ms: number) => void;
  } = $props();

  let container = $state<HTMLDivElement | null>(null);
  /** 是否跟随播放自动滚动；用户手动滚动后暂停一会儿 */
  let following = $state(true);
  let resumeTimer: ReturnType<typeof setTimeout> | null = null;

  const lines = $derived(lyric?.lines ?? []);

  function pauseFollowing() {
    following = false;
    if (resumeTimer) clearTimeout(resumeTimer);
    resumeTimer = setTimeout(() => {
      following = true;
    }, 5000);
  }

  function resumeFollowing() {
    following = true;
    if (resumeTimer) {
      clearTimeout(resumeTimer);
      resumeTimer = null;
    }
  }

  /** 拖动滚动：只用 pointer 事件改 scrollTop，和自动跟随互斥 */
  let dragging = false;
  let moved = false;
  let dragStartY = 0;
  let dragStartTop = 0;

  function onPointerDown(event: PointerEvent) {
    if (!container || event.button !== 0) return;
    dragging = true;
    moved = false;
    dragStartY = event.clientY;
    dragStartTop = container.scrollTop;
  }

  function onPointerMove(event: PointerEvent) {
    if (!dragging || !container) return;
    const delta = event.clientY - dragStartY;
    if (Math.abs(delta) > 3) moved = true;
    if (!moved) return;
    pauseFollowing();
    container.scrollTop = dragStartTop - delta;
  }

  function onPointerUp() {
    dragging = false;
    // 让点击处理先看到 moved，再复位
    setTimeout(() => (moved = false), 0);
  }

  /** 点歌词行 = 跳到这一句 */
  function chooseLine(index: number) {
    if (moved) return;
    const line = lines[index];
    if (!line) return;
    onseek?.(line.startMs);
    resumeFollowing();
  }

  // 与 Rust 侧同一个二分：找到最后一个 startMs <= position 的行
  const activeIndex = $derived.by(() => {
    const list = lines;
    let low = 0;
    let high = list.length - 1;
    let answer = -1;
    while (low <= high) {
      const mid = (low + high) >> 1;
      if (list[mid].startMs <= positionMs) {
        answer = mid;
        low = mid + 1;
      } else {
        high = mid - 1;
      }
    }
    return answer;
  });

  $effect(() => {
    const index = activeIndex;
    if (!container || index < 0 || !following) return;
    const element = container.querySelector<HTMLElement>(`[data-line="${index}"]`);
    if (!element) return;

    // 注意：不能用 scrollIntoView —— 它会连带滚动 body/html，把整个界面带着走。
    // 这里只改歌词容器自己的 scrollTop。
    const box = container.getBoundingClientRect();
    const line = element.getBoundingClientRect();
    const delta = line.top - box.top - (box.height - line.height) / 2;
    container.scrollTo({ top: container.scrollTop + delta, behavior: 'smooth' });
  });
</script>

<aside class="lyrics" class:stage={variant === 'stage'}>
  <header>
    <span>歌词</span>
    {#if lyric?.hasYrc}<span class="tag">逐字</span>{/if}
  </header>

  <div
    class="lines"
    class:dragging
    bind:this={container}
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onpointercancel={onPointerUp}
    onwheel={pauseFollowing}
  >
    {#if lines.length === 0}
      <p class="empty">暂无歌词</p>
    {:else}
      {#each lines as line, index (index)}
        <p
          class:active={index === activeIndex}
          data-line={index}
          role="button"
          tabindex="0"
          title="跳到这一句"
          onclick={() => chooseLine(index)}
          onkeydown={(event) => {
            if (event.key === 'Enter' || event.key === ' ') {
              event.preventDefault();
              chooseLine(index);
            }
          }}
        >
          <span class="text">{line.text}</span>
          {#if line.translation}<span class="trans">{line.translation}</span>{/if}
        </p>
      {/each}
    {/if}
  </div>

  {#if !following && lines.length > 0}
    <button class="resume" onclick={resumeFollowing}>回到当前</button>
  {/if}
</aside>

<style>
  .lyrics {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    background: var(--surface-1);
    border-left: 1px solid var(--border);
  }

  /* 现在播放页里的歌词：居中、没有边框和底色，字号更大 */
  .lyrics.stage {
    background: transparent;
    border-left: none;
  }

  .lyrics.stage header {
    display: none;
  }

  .lyrics.stage .lines {
    padding: 8px 10% 48px;
  }

  .lyrics.stage .lines p {
    font-size: 17px;
    padding: 10px 0;
    color: var(--text-dim);
    text-align: center;
    transform-origin: center center;
  }

  .lyrics.stage .lines p.active {
    color: var(--text);
    transform: scale(1.02);
  }

  .lyrics.stage .lines p.active .text {
    font-weight: 700;
  }

  .lyrics.stage .trans {
    font-size: 13px;
    text-align: center;
  }

  header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 40px 18px 8px;
    font-size: 12px;
    color: var(--text-dim);
  }

  .tag {
    font-size: 10px;
    padding: 1px 6px;
    border-radius: 999px;
    color: var(--accent);
    border: 1px solid color-mix(in srgb, var(--accent) 40%, transparent);
  }

  .lines {
    flex: 1;
    min-height: 0;
    min-width: 0;
    /* 只允许纵向滚动。只写 overflow-y 会让 overflow-x 一起变成 auto，
       一丁点超宽就会出现横向滚动条——这就是播放页底部那条白杠。 */
    overflow: hidden auto;
    overscroll-behavior: contain;
    padding: 8px 18px 32px;
    scroll-behavior: smooth;
  }

  .lines p {
    margin: 0;
    padding: 7px 0;
    font-size: 13px;
    line-height: 1.5;
    color: var(--text-dim);
    overflow-wrap: anywhere;
    transition:
      color 180ms ease,
      transform 180ms ease;
    transform-origin: left center;
  }

  .lines p.active {
    color: var(--text);
    transform: scale(1.03);
  }

  .lines p.active .text {
    font-weight: 600;
  }

  .trans {
    display: block;
    font-size: 11px;
    color: var(--text-dim);
    margin-top: 2px;
  }

  .empty {
    color: var(--text-dim);
    font-size: 12px;
  }
  /* ===== 字体与双语层级 ===== */
  .lines p {
    font-family: var(--font-ui);
    font-size: 13px;
    line-height: 1.7;
    color: var(--text-faint);
  }

  .lines p.active {
    color: var(--text);
  }

  .lines p.active .text {
    font-weight: 600;
  }

  .trans {
    display: block;
    margin-top: 4px;
    font-family: var(--font-ui);
    font-size: 11.5px;
    line-height: 1.5;
    color: var(--text-faint);
  }

  .lyrics.stage .lines p {
    font-size: 18px;
    line-height: 1.75;
    padding: 10px 0;
  }

  .lyrics.stage .lines p.active {
    transform: none;
  }

  .lyrics.stage .lines p.active .text {
    font-weight: 600;
  }

  .lyrics.stage .trans {
    margin-top: 5px;
    font-size: 12px;
    color: var(--text-faint);
  }

  header {
    font-family: var(--font-ui);
    font-size: 11px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-faint);
  }

  .tag {
    font-family: var(--font-ui);
    font-size: 10px;
    letter-spacing: 0.06em;
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 35%, transparent);
  }  /* ===== 歌词交互：可拖动、可点击跳转 ===== */
  .lyrics {
    position: relative;
  }

  .lines {
    cursor: grab;
    user-select: none;
  }

  .lines.dragging {
    cursor: grabbing;
  }

  .lines.dragging p {
    pointer-events: none;
  }

  .lines p[role='button'] {
    cursor: pointer;
    transition:
      color 160ms ease,
      transform 160ms ease,
      opacity 160ms ease;
  }

  .lines p[role='button']:hover {
    color: var(--text);
  }

  .lines p:active {
    opacity: 0.7;
  }

  /* 手动滚动后出现的「回到当前」 */
  .resume {
    position: absolute;
    left: 50%;
    bottom: 14px;
    transform: translateX(-50%);
    height: 28px;
    padding: 0 14px;
    border: none;
    border-radius: 999px;
    background: color-mix(in srgb, var(--text) 12%, transparent);
    color: var(--text);
    font-size: 12px;
    cursor: pointer;
    backdrop-filter: none;
  }

  .resume:hover {
    background: color-mix(in srgb, var(--text) 18%, transparent);
  }

  .lyrics.stage .resume {
    bottom: 18px;
  }</style>







