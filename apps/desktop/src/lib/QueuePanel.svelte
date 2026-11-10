<script lang="ts">
  import { coverUrl, jumpTo, moveQueueItem, queueItems } from './api';
  import type { Track } from './types';

  let {
    open,
    currentId = null,
    onclose,
  }: {
    open: boolean;
    currentId?: number | null;
    onclose: () => void;
  } = $props();

  /** 行高固定，虚拟滚动依赖它 */
  const ROW_H = 56;
  const OVERSCAN = 6;

  let tracks = $state<Track[]>([]);
  let position = $state(0);
  let loading = $state(false);
  let viewport = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let viewportH = $state(600);
  let jumping = $state<number | null>(null);
  /** 拖动排序状态 */
  let dragIndex = $state<number | null>(null);
  let dropTarget = $state<number | null>(null);
  let dragging = $state(false);
  let suppressClick = false;
  let dragStartY = 0;

  const total = $derived(tracks.length);
  const start = $derived(Math.max(0, Math.floor(scrollTop / ROW_H) - OVERSCAN));
  const end = $derived(Math.min(total, Math.ceil((scrollTop + viewportH) / ROW_H) + OVERSCAN));
  const visible = $derived(tracks.slice(start, end));

  async function refresh() {
    try {
      const data = await queueItems();
      tracks = data.tracks;
      position = data.position;
    } catch {
      // 队列为空或后端未就绪时保持原样
    } finally {
      loading = false;
    }
  }

  // 打开面板、切歌时拉一次最新顺序
  $effect(() => {
    if (!open) return;
    void currentId;
    loading = tracks.length === 0;
    void refresh();
  });

  $effect(() => {
    const element = viewport;
    if (!element) return;
    viewportH = element.clientHeight;
    const observer = new ResizeObserver(() => {
      viewportH = element.clientHeight;
    });
    observer.observe(element);
    return () => observer.disconnect();
  });

  // 打开时把当前曲目滚到中间
  $effect(() => {
    if (!open || !viewport || position <= 0) return;
    viewport.scrollTop = Math.max(0, position * ROW_H - viewportH / 2 + ROW_H);
  });

  async function jump(index: number) {
    if (jumping !== null) return;
    jumping = index;
    try {
      await jumpTo(index);
      await refresh();
    } catch {
      // 跳转失败保持原状
    } finally {
      jumping = null;
    }
  }

  /** 拖动排序：pointer 事件 + 插入位置线，虚拟列表里按绝对下标算 */
  function onRowPointerDown(event: PointerEvent, index: number) {
    if (event.button !== 0) return;
    dragIndex = index;
    dragStartY = event.clientY;
    dragging = false;
    try {
      viewport?.setPointerCapture(event.pointerId);
    } catch {
      // 合成事件没有真实 pointerId
    }
  }

  function onPointerMove(event: PointerEvent) {
    if (dragIndex == null || !viewport) return;
    const delta = event.clientY - dragStartY;
    if (!dragging && Math.abs(delta) < 5) return;
    dragging = true;

    const rect = viewport.getBoundingClientRect();
    const y = event.clientY - rect.top + viewport.scrollTop;
    dropTarget = Math.max(0, Math.min(total, Math.round(y / ROW_H)));

    // 靠近上下边缘时自动滚动，不然拖不进视口外的地方
    const edge = 30;
    const offset = event.clientY - rect.top;
    if (offset < edge) viewport.scrollTop -= 10;
    else if (rect.height - offset < edge) viewport.scrollTop += 10;
  }

  async function onPointerUp(event: PointerEvent) {
    if (dragIndex == null) return;
    const from = dragIndex;
    dragIndex = null;
    try {
      viewport?.releasePointerCapture(event.pointerId);
    } catch {
      // 同上
    }
    if (!dragging) return;

    suppressClick = true;
    const target = dropTarget ?? from;
    dropTarget = null;
    dragging = false;

    // 插入位置在目标行之前；往后拖要减一
    let to = target > from ? target - 1 : target;
    to = Math.max(0, Math.min(total - 1, to));
    if (to === from) return;

    try {
      await moveQueueItem(from, to);
      await refresh();
    } catch {
      // 失败保持原状
    }
    setTimeout(() => (suppressClick = false), 0);
  }

  /** 键盘也能调整顺序 */
  async function nudge(index: number, delta: number) {
    const to = index + delta;
    if (to < 0 || to >= total) return;
    try {
      await moveQueueItem(index, to);
      await refresh();
    } catch {
      // 忽略
    }
  }

  function formatDuration(ms: number): string {
    const second = Math.floor(ms / 1000);
    return `${Math.floor(second / 60)}:${String(second % 60).padStart(2, '0')}`;
  }
</script>

<aside class="queue" class:open aria-hidden={!open}>
  <header>
    <span class="label">待播清单</span>
    <span class="count">{total > 0 ? `${position + 1} / ${total}` : '空'}</span>
    <button class="close" aria-label="关闭待播清单" title="关闭" onclick={onclose}>✕</button>
  </header>

  {#if total === 0}
    <div class="empty">
      <p>{loading ? '正在读取队列…' : '队列是空的'}</p>
      <p class="dim">点歌单或搜索里的任意一首开始播放</p>
    </div>
  {:else}
    <div
      class="viewport"
      class:dragging
      bind:this={viewport}
      onscroll={(event) => (scrollTop = event.currentTarget.scrollTop)}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onpointercancel={onPointerUp}
    >
      <div class="spacer" style:height={`${total * ROW_H}px`}>
        {#if dragging && dropTarget !== null}
          <div class="drop-line" style:top={`${dropTarget * ROW_H}px`}></div>
        {/if}
        <ul style:transform={`translateY(${start * ROW_H}px)`}>
          {#each visible as track, offset (track.id)}
            {@const index = start + offset}
            <li
              class:current={index === position}
              class:dragging={dragging && dragIndex === index}
              class:drop-before={dragging && dropTarget === index}
            >
              <button
                class="row"
                title={track.name}
                onpointerdown={(event) => onRowPointerDown(event, index)}
                onclick={() => {
                  if (suppressClick) return;
                  jump(index);
                }}
                onkeydown={(event) => {
                  if (!event.altKey) return;
                  if (event.key === 'ArrowUp') {
                    event.preventDefault();
                    void nudge(index, -1);
                  } else if (event.key === 'ArrowDown') {
                    event.preventDefault();
                    void nudge(index, 1);
                  }
                }}
              >
                <span class="index">
                  {#if index === position}
                    <span class="eq" aria-label="正在播放"><i></i><i></i><i></i></span>
                  {:else}
                    {String(index + 1).padStart(2, '0')}
                  {/if}
                </span>
                {#if coverUrl(track.coverUrl, 80)}
                  <img class="thumb" src={coverUrl(track.coverUrl, 80)} alt="" loading="lazy" decoding="async" />
                {:else}
                  <span class="thumb placeholder">{track.name.slice(0, 1)}</span>
                {/if}
                <span class="main">
                  <span class="title">{track.name}</span>
                  <span class="meta">{track.artists.join(' / ')}</span>
                </span>
                <span class="duration">{formatDuration(track.durationMs)}</span>
              </button>
            </li>
          {/each}
        </ul>
      </div>
    </div>
  {/if}
</aside>

<style>
  .queue {
    position: fixed;
    top: 0;
    right: 0;
    bottom: var(--player-h);
    width: 340px;
    z-index: 30;
    display: flex;
    flex-direction: column;
    background: var(--surface-1);
    border-left: none;
    box-shadow: var(--shadow-2);
    transform: translateX(100%);
    opacity: 0;
    pointer-events: none;
    transition:
      transform 200ms cubic-bezier(0.2, 0.7, 0.3, 1),
      opacity 160ms ease;
  }

  .queue.open {
    transform: translateX(0);
    opacity: 1;
    pointer-events: auto;
  }

  header {
    flex: none;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 44px 14px 10px 18px;
  }

  .label {
    font-size: 11px;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--text-faint);
  }

  .count {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-faint);
  }

  .close {
    margin-left: auto;
    width: 26px;
    height: 26px;
    display: grid;
    place-items: center;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: var(--text-faint);
    cursor: pointer;
    font-size: 13px;
    transition:
      background 120ms ease,
      color 120ms ease;
  }

  .close:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .viewport {
    flex: 1;
    min-height: 0;
    overflow: hidden auto;
  }

  .spacer {
    position: relative;
    width: 100%;
  }

  ul {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    height: 56px;
  }

  .row {
    width: 100%;
    height: 100%;
    display: grid;
    grid-template-columns: 26px 36px minmax(0, 1fr) 40px;
    align-items: center;
    gap: 10px;
    padding: 0 14px 0 16px;
    border: none;
    background: transparent;
    color: inherit;
    text-align: left;
    cursor: pointer;
    transition: background 140ms ease;
  }

  .row:hover {
    background: var(--surface-2);
  }

  li.current .row {
    background: color-mix(in srgb, var(--accent) 6%, transparent);
    box-shadow: inset 2px 0 0 var(--accent);
  }

  .index {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-faint);
    font-variant-numeric: tabular-nums;
  }

  li.current .index {
    color: var(--accent);
  }

  .eq {
    display: inline-flex;
    align-items: flex-end;
    gap: 2px;
    height: 10px;
  }

  .eq i {
    width: 2px;
    height: 3px;
    background: var(--accent);
    animation: eq 900ms ease-in-out infinite;
  }

  .eq i:nth-child(2) {
    animation-delay: 150ms;
  }

  .eq i:nth-child(3) {
    animation-delay: 300ms;
  }

  @keyframes eq {
    0%,
    100% {
      height: 3px;
    }
    50% {
      height: 10px;
    }
  }

  .thumb {
    width: 36px;
    height: 36px;
    border-radius: 4px;
    object-fit: cover;
    background: var(--surface-2);
  }

  .thumb.placeholder {
    display: grid;
    place-items: center;
    color: var(--text-faint);
    font-size: 13px;
  }

  .main {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .title {
    font-size: 13px;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  li:not(.current) .title {
    color: var(--text-dim);
  }

  .meta {
    font-size: 11px;
    color: var(--text-faint);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .duration {
    justify-self: end;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-faint);
    font-variant-numeric: tabular-nums;
  }

  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    justify-content: center;
    align-items: center;
    gap: 6px;
    padding: 0 24px;
    font-size: 13px;
    color: var(--text-dim);
    text-align: center;
  }

  .empty .dim {
    font-size: 12px;
    color: var(--text-faint);
  }
  /* ===== 拖动排序 ===== */
  .viewport.dragging {
    cursor: grabbing;
    user-select: none;
  }

  li.dragging .row {
    opacity: 0.55;
  }

  li.drop-before .row {
    box-shadow: inset 0 2px 0 var(--accent);
  }

  .drop-line {
    position: absolute;
    left: 14px;
    right: 14px;
    height: 2px;
    border-radius: 2px;
    background: var(--accent);
    pointer-events: none;
    z-index: 2;
  }

  .drop-line::before {
    content: '';
    position: absolute;
    left: -3px;
    top: -2px;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
  }</style>
