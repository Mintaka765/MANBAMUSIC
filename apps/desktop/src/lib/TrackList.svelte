<script lang="ts">
  import { onMount } from 'svelte';
  import { coverUrl } from './api';
  import type { Track } from './types';

  let {
    tracks,
    currentId = null,
    playing = true,
    onplay,
    onselect,
    onalbum,
    onartist,
    oncontext,
  }: {
    tracks: Track[];
    currentId?: number | null;
    playing?: boolean;
    onplay: (track: Track, index: number) => void;
    onselect?: (track: Track, index: number) => void;
    /** 点专辑名进专辑页 */
    onalbum?: (track: Track) => void;
    /** 点歌手名进歌手页 */
    onartist?: (id: number) => void;
    /** 右键菜单 */
    oncontext?: (track: Track, index: number, event: MouseEvent) => void;
  } = $props();

  /** 单击只选中（给右键菜单留位置），双击才播放 */
  let selectedId = $state<number | null>(null);

  function select(track: Track, index: number) {
    selectedId = track.id;
    onselect?.(track, index);
  }

  /** 行高 = 64px（含底部发丝线）。虚拟滚动依赖它精确，改样式要同步改这里。 */
  const ROW_H = 60;
  const OVERSCAN = 8;

  let viewport = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let viewportH = $state(600);
  let animate = $state(true);

  const total = $derived(tracks.length);
  const start = $derived(Math.max(0, Math.floor(scrollTop / ROW_H) - OVERSCAN));
  const end = $derived(Math.min(total, Math.ceil((scrollTop + viewportH) / ROW_H) + OVERSCAN));
  const visible = $derived(tracks.slice(start, end));

  function onScroll(event: Event) {
    scrollTop = (event.currentTarget as HTMLElement).scrollTop;
  }

  function formatDuration(ms: number): string {
    const total = Math.floor(ms / 1000);
    return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, '0')}`;
  }

  function feeLabel(fee: number): string {
    // 网易的 fee 语义：0=免费 8=标准音质免费 1=VIP 4=需购买
    if (fee === 0) return '';
    if (fee === 8) return '';
    if (fee === 1) return 'VIP';
    if (fee === 4) return '付费';
    return '';
  }

  onMount(() => {
    const timer = setTimeout(() => (animate = false), 600);
    return () => clearTimeout(timer);
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

  // 换歌单 / 换搜索结果时清空选中并回到顶部
  $effect(() => {
    void tracks;
    if (viewport && viewport.scrollTop !== 0) {
      viewport.scrollTop = 0;
    }
    selectedId = null;
    scrollTop = 0;
  });
</script>

{#if total > 0}
  <div class="viewport" bind:this={viewport} onscroll={onScroll}>
    <div class="spacer" style:height={`${total * ROW_H}px`}>
      <ul class="tracks" style:transform={`translateY(${start * ROW_H}px)`}>
        {#each visible as track, offset (start + offset)}
          {@const index = start + offset}
          <li class:active={track.id === currentId} class:selected={track.id === selectedId}>
            <button
              class="row"
              class:animated={animate && index < 24}
              class:paused={!playing}
              style:--i={Math.min(index, 12)}
              onclick={() => select(track, index)}
              ondblclick={() => onplay(track, index)}
              oncontextmenu={(event) => {
                event.preventDefault();
                select(track, index);
                oncontext?.(track, index, event);
              }}
              onkeydown={(event) => {
                if (event.key === 'Enter') onplay(track, index);
              }}
            >
              <span class="index">
                {#if track.id === currentId}
                  <span class="eq" aria-label="正在播放"><i></i><i></i><i></i></span>
                {:else}
                  {String(index + 1).padStart(2, '0')}
                {/if}
              </span>
              {#if coverUrl(track.coverUrl, 120)}
                <img
                  class="thumb"
                  src={coverUrl(track.coverUrl, 120)}
                  alt=""
                  loading="lazy"
                  decoding="async"
                />
              {:else}
                <span class="thumb placeholder">{track.name.slice(0, 1)}</span>
              {/if}
              <span class="main">
                <span class="title">{track.name}</span>
                <span class="meta">
                  {#each track.artists as artist, index (index)}
                    {#if index > 0}<span class="sep"> / </span>{/if}
                    {#if onartist && track.artistIds?.[index]}
                      <span
                        class="artist-link"
                        role="link"
                        tabindex="-1"
                        title="查看歌手"
                        onclick={(event) => {
                          event.stopPropagation();
                          onartist(track.artistIds?.[index] ?? 0);
                        }}
                      >{artist}</span>
                    {:else}
                      {artist}
                    {/if}
                  {/each}
                </span>
              </span>
              {#if track.albumId && onalbum}
                <span
                  class="album link"
                  role="link"
                  tabindex="-1"
                  title="查看专辑"
                  onclick={(event) => {
                    event.stopPropagation();
                    onalbum(track);
                  }}
                >{track.album}</span>
              {:else}
                <span class="album">{track.album}</span>
              {/if}
              {#if feeLabel(track.fee)}
                <span class="badge" data-fee={track.fee}>{feeLabel(track.fee)}</span>
              {:else}
                <span></span>
              {/if}
              <span class="duration">{formatDuration(track.durationMs)}</span>
            </button>
          </li>
        {/each}
      </ul>
    </div>
  </div>
{/if}

<style>
  .viewport {
    flex: 1;
    min-height: 0;
    overflow: hidden auto;
  }

  .spacer {
    position: relative;
    width: 100%;
  }

  .tracks {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .tracks li {
    height: 60px;
  }

  .row {
    width: 100%;
    height: 100%;
    display: grid;
    grid-template-columns: 40px 40px minmax(0, 1.5fr) minmax(0, 1fr) 56px 64px;
    align-items: center;
    gap: 16px;
    padding: 0 28px;
    border: none;
    border-radius: 0;
    background: transparent;
    color: inherit;
    text-align: left;
    cursor: pointer;
    transition: background 160ms ease;
  }

  .row.animated {
    animation: row-in 220ms cubic-bezier(0.2, 0.7, 0.3, 1) both;
    animation-delay: calc(var(--i, 0) * 18ms);
  }

  @keyframes row-in {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .row:hover {
    background: var(--surface-2);
  }

  /* 单击选中态：比 hover 轻，且不与"正在播放"抢强调 */
  li.selected:not(.active) .row {
    background: color-mix(in srgb, var(--text) 5%, transparent);
  }

  li.selected:not(.active) .title {
    color: var(--text);
  }

  li.active .row {
    background: color-mix(in srgb, var(--accent) 6%, transparent);
    box-shadow: inset 2px 0 0 var(--accent);
  }

  .index {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-faint);
    font-variant-numeric: tabular-nums;
  }

  li.active .index {
    color: var(--accent);
  }

  .eq {
    display: inline-flex;
    align-items: flex-end;
    gap: 2px;
    height: 12px;
  }

  .eq i {
    width: 2px;
    height: 4px;
    background: var(--accent);
    animation: eq 900ms ease-in-out infinite;
  }

  .eq i:nth-child(2) {
    animation-delay: 150ms;
  }

  .eq i:nth-child(3) {
    animation-delay: 300ms;
  }

  .row.paused .eq i {
    animation-play-state: paused;
    height: 6px;
  }

  @keyframes eq {
    0%,
    100% {
      height: 4px;
    }
    50% {
      height: 12px;
    }
  }

  .thumb {
    width: 40px;
    height: 40px;
    border-radius: 5px;
    object-fit: cover;
    background: var(--surface-2);
    transition: transform 180ms cubic-bezier(0.2, 0.7, 0.3, 1);
  }

  .row:hover .thumb {
    transform: scale(1.04);
  }

  .thumb.placeholder {
    display: grid;
    place-items: center;
    color: var(--text-faint);
    font-family: var(--font-display);
    font-size: 16px;
  }

  .main {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }

  .title {
    font-family: var(--font-ui);
    font-size: 14px;
    font-weight: 500;
    line-height: 1.25;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  li:not(.active) .title {
    color: var(--text-dim);
  }

  .row:hover .title {
    color: var(--text);
  }

  .meta {
    font-size: 12px;
    color: var(--text-faint);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .album {
    font-size: 12px;
    color: var(--text-faint);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .badge {
    justify-self: end;
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--warning);
  }

  .duration {
    justify-self: end;
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--text-faint);
    font-variant-numeric: tabular-nums;
  }
  .album.link {
    cursor: pointer;
    transition: color 140ms ease;
  }

  .album.link:hover {
    color: var(--text);
    text-decoration: underline;
    text-underline-offset: 2px;
  }  .meta .artist-link {
    cursor: pointer;
    transition: color 140ms ease;
  }

  .meta .artist-link:hover {
    color: var(--text);
    text-decoration: underline;
    text-underline-offset: 2px;
  }</style>





