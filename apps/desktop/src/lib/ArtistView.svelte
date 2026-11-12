<script lang="ts">
  import { coverUrl } from './api';
  import TrackList from './TrackList.svelte';
  import type { ArtistAlbum, ArtistDetail, Track } from './types';

  let {
    artist,
    albums = null,
    tab = 'songs',
    loadingAlbums = false,
    currentId = null,
    playing = true,
    onback,
    ontab,
    onalbum,
    onplay,
    onplayall,
  }: {
    artist: ArtistDetail;
    albums?: ArtistAlbum[] | null;
    tab?: 'songs' | 'albums';
    loadingAlbums?: boolean;
    currentId?: number | null;
    playing?: boolean;
    onback: () => void;
    ontab: (tab: 'songs' | 'albums') => void;
    onalbum: (id: number) => void;
    onplay: (track: Track, index: number) => void;
    onplayall: () => void;
  } = $props();

  function year(ms: number | null): string {
    return ms ? String(new Date(ms).getFullYear()) : '';
  }
</script>

<div class="artist-head">
  <button class="back" aria-label="返回" title="返回" onclick={onback}>←</button>
  {#if coverUrl(artist.coverUrl, 300)}
    <img
      class="avatar"
      src={coverUrl(artist.coverUrl, 300)}
      alt=""
      loading="lazy"
      decoding="async"
    />
  {:else}
    <div class="avatar placeholder">♪</div>
  {/if}
  <div class="info">
    <p class="kind">歌手</p>
    <h1 class="name">{artist.name}</h1>
    <p class="meta">单曲 {artist.songCount} · 专辑 {artist.albumCount}</p>
  </div>
  <button class="play" onclick={onplayall} disabled={artist.hotSongs.length === 0}>播放热门</button>
</div>

<nav class="tabs" aria-label="歌手内容">
  <button class:on={tab === 'songs'} onclick={() => ontab('songs')}>热门歌曲</button>
  <button class:on={tab === 'albums'} onclick={() => ontab('albums')}>专辑</button>
</nav>

{#if tab === 'songs'}
  <TrackList
    tracks={artist.hotSongs}
    {currentId}
    {playing}
    {onplay}
    onalbum={(track) => track.albumId && onalbum(track.albumId)}
  />
{:else if loadingAlbums}
  <p class="hint">正在读取专辑…</p>
{:else if albums && albums.length > 0}
  <div class="albums">
    {#each albums as album (album.id)}
      <button class="album-card" title={album.name} onclick={() => onalbum(album.id)}>
        {#if coverUrl(album.coverUrl, 300)}
          <img
            class="album-cover"
            src={coverUrl(album.coverUrl, 300)}
            alt=""
            loading="lazy"
            decoding="async"
          />
        {:else}
          <div class="album-cover placeholder">♪</div>
        {/if}
        <span class="album-name">{album.name}</span>
        <span class="album-sub">
          {year(album.publishMs)}{year(album.publishMs) ? ' · ' : ''}{album.size} 首
        </span>
      </button>
    {/each}
  </div>
{:else}
  <p class="hint">还没有专辑</p>
{/if}

<style>
  .artist-head {
    display: flex;
    align-items: flex-end;
    gap: 20px;
    padding: 8px 4px 18px;
  }

  .back {
    width: 32px;
    height: 32px;
    align-self: flex-start;
    margin-top: 6px;
    flex: none;
    border: none;
    border-radius: 50%;
    background: color-mix(in srgb, var(--text) 8%, transparent);
    color: var(--text);
    font-size: 15px;
    cursor: pointer;
    display: grid;
    place-items: center;
  }

  .back:hover {
    background: color-mix(in srgb, var(--text) 14%, transparent);
  }

  .avatar {
    width: 132px;
    height: 132px;
    border-radius: 50%;
    object-fit: cover;
    background: var(--surface-2);
    flex: none;
  }

  .avatar.placeholder {
    display: grid;
    place-items: center;
    font-size: 36px;
    color: var(--text-faint);
  }

  .info {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-bottom: 4px;
  }

  .kind {
    margin: 0;
    font-size: 11px;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--text-faint);
  }

  .name {
    margin: 0;
    font-size: 26px;
    line-height: 1.25;
    font-weight: 650;
    letter-spacing: -0.01em;
    color: var(--text);
    overflow-wrap: anywhere;
  }

  .meta {
    margin: 0;
    font-size: 12px;
    color: var(--text-faint);
  }

  .play {
    margin-left: auto;
    height: 36px;
    padding: 0 20px;
    border: none;
    border-radius: 999px;
    background: var(--accent);
    color: var(--accent-contrast, #fff);
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    flex: none;
    margin-bottom: 4px;
  }

  .play:hover:not(:disabled) {
    filter: brightness(1.05);
  }

  .play:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .tabs {
    display: flex;
    gap: 18px;
    padding: 0 4px 10px;
  }

  .tabs button {
    position: relative;
    padding: 6px 0;
    border: none;
    background: none;
    color: var(--text-dim);
    font-size: 13px;
    cursor: pointer;
  }

  .tabs button:hover {
    color: var(--text);
  }

  .tabs button.on {
    color: var(--text);
    font-weight: 600;
  }

  .tabs button.on::after {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 2px;
    border-radius: 2px;
    background: var(--accent);
  }

  .hint {
    margin: 18px 4px;
    font-size: 13px;
    color: var(--text-faint);
  }

  .albums {
    flex: 1;
    min-height: 0;
    overflow: hidden auto;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 18px 16px;
    padding: 4px 4px 80px;
  }

  .album-card {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 0;
    border: none;
    background: none;
    text-align: left;
    cursor: pointer;
    min-width: 0;
  }

  .album-cover {
    width: 100%;
    aspect-ratio: 1;
    border-radius: 10px;
    object-fit: cover;
    background: var(--surface-2);
  }

  .album-cover.placeholder {
    display: grid;
    place-items: center;
    font-size: 32px;
    color: var(--text-faint);
  }

  .album-card:hover .album-cover {
    filter: brightness(1.05);
  }

  .album-name {
    font-size: 13px;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .album-sub {
    font-size: 11px;
    color: var(--text-faint);
  }
</style>
