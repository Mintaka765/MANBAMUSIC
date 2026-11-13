<script lang="ts">
  import { coverUrl } from './api';
  import type { Playlist, Track } from './types';

  let {
    track,
    playlists,
    onchoose,
    onclose,
  }: {
    track: Track;
    playlists: Playlist[];
    onchoose: (playlist: Playlist) => void;
    onclose: () => void;
  } = $props();

  /** 特殊歌单（我喜欢的音乐）不能用「加入歌单」写入，收藏走红心 */
  const choices = $derived(playlists.filter((item) => item.specialType !== 5));

  function pick(playlist: Playlist) {
    onchoose(playlist);
    onclose();
  }
</script>

<div class="overlay" role="presentation" onclick={onclose}>
  <div
    class="panel"
    role="dialog"
    aria-modal="true"
    aria-label="收藏到歌单"
    onclick={(event) => event.stopPropagation()}
  >
    <header>
      <p class="title">收藏到歌单</p>
      <p class="sub" title={track.name}>{track.name}</p>
    </header>

    {#if choices.length === 0}
      <p class="empty">还没有自己的歌单</p>
    {:else}
      <div class="list">
        {#each choices as playlist (playlist.id)}
          <button class="row" title={playlist.name} onclick={() => pick(playlist)}>
            {#if coverUrl(playlist.coverUrl, 80)}
              <img src={coverUrl(playlist.coverUrl, 80)} alt="" loading="lazy" decoding="async" />
            {:else}
              <span class="thumb">♪</span>
            {/if}
            <span class="name">{playlist.name}</span>
            <span class="count">{playlist.trackCount}</span>
          </button>
        {/each}
      </div>
    {/if}

    <footer>
      <button class="ghost" onclick={onclose}>取消</button>
    </footer>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 70;
    display: grid;
    place-items: center;
    background: rgba(0, 0, 0, 0.36);
  }

  .panel {
    width: 380px;
    max-height: 70vh;
    display: flex;
    flex-direction: column;
    padding: 16px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface-2);
    box-shadow: var(--shadow-2, 0 18px 48px rgba(0, 0, 0, 0.32));
  }

  header {
    padding: 0 2px 12px;
  }

  .title {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: var(--text);
  }

  .sub {
    margin: 4px 0 0;
    font-size: 12px;
    color: var(--text-faint);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .list {
    flex: 1;
    min-height: 0;
    overflow: hidden auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 44px;
    padding: 0 8px;
    border: none;
    border-radius: 8px;
    background: none;
    color: var(--text-dim);
    font-size: 13px;
    text-align: left;
    cursor: pointer;
  }

  .row:hover {
    background: color-mix(in srgb, var(--text) 7%, transparent);
    color: var(--text);
  }

  .row img,
  .thumb {
    width: 28px;
    height: 28px;
    border-radius: 6px;
    object-fit: cover;
    background: var(--surface-1);
    flex: none;
  }

  .thumb {
    display: grid;
    place-items: center;
    color: var(--text-faint);
    font-size: 13px;
  }

  .name {
    flex: 1;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .count {
    font-size: 11px;
    color: var(--text-faint);
  }

  .empty {
    margin: 24px 0;
    text-align: center;
    font-size: 13px;
    color: var(--text-faint);
  }

  footer {
    display: flex;
    justify-content: flex-end;
    padding-top: 10px;
  }

  .ghost {
    height: 30px;
    padding: 0 14px;
    border: none;
    border-radius: 8px;
    background: color-mix(in srgb, var(--text) 8%, transparent);
    color: var(--text-dim);
    font-size: 12px;
    cursor: pointer;
  }

  .ghost:hover {
    color: var(--text);
  }
</style>
