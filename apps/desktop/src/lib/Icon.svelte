<script lang="ts">
  export type IconName =
    | 'prev'
    | 'next'
    | 'play'
    | 'pause'
    | 'repeat'
    | 'repeatOne'
    | 'sequential'
    | 'shuffle'
    | 'lyrics'
    | 'volume'
    | 'home'
    | 'song'
    | 'album'
    | 'artist'
    | 'heart'
    | 'plus'
    | 'settings'
    | 'queue'
    | 'radio'
    | 'radar';

  let { name, size = 16 }: { name: IconName; size?: number } = $props();

  const solid = $derived(
    name === 'play' || name === 'pause' || name === 'prev' || name === 'next',
  );
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill={solid ? 'currentColor' : 'none'}
  stroke={solid ? 'none' : 'currentColor'}
  stroke-width="1.7"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  class="ico"
>
  {#if name === 'prev'}
    <path d="M7 5h2.2v14H7z" />
    <path d="M19.5 5v14L9.8 12z" />
  {:else if name === 'next'}
    <path d="M14.8 5H17v14h-2.2z" />
    <path d="M4.5 5v14L14.2 12z" />
  {:else if name === 'play'}
    <path d="M8 5.2v13.6L19.2 12z" />
  {:else if name === 'pause'}
    <path d="M7 5h3.4v14H7z" />
    <path d="M13.6 5H17v14h-3.4z" />
  {:else if name === 'sequential'}
  <!-- 顺序播放：一圈方框 + 左上角指示箭头（不循环） -->
  <path d="M4 7.5v9" />
  <path d="M8 12h9.5" />
  <path d="M14.2 8.6 17.6 12l-3.4 3.4" />
{:else if name === 'repeat' || name === 'repeatOne'}
    <path d="M17 2.5l3.5 3.5L17 9.5" />
    <path d="M3.5 11.5V9a4 4 0 0 1 4-4h13" />
    <path d="M7 21.5L3.5 18 7 14.5" />
    <path d="M20.5 12.5V15a4 4 0 0 1-4 4h-13" />
    {#if name === 'repeatOne'}
      <text
        x="12"
        y="15.6"
        font-size="7"
        font-weight="700"
        text-anchor="middle"
        fill="currentColor"
        stroke="none">1</text
      >
    {/if}
  {:else if name === 'shuffle'}
    <path d="M16 3h5v5" />
    <path d="M4 20 21 3" />
    <path d="M21 16v5h-5" />
    <path d="M15 15l6 6" />
    <path d="M4 4l5 5" />
  {:else if name === 'lyrics'}
    <path d="M21 11.5a8.4 8.4 0 0 1-9 8.4 9.9 9.9 0 0 1-4-.8L3 21l1.9-4.6A8.4 8.4 0 0 1 12 3.1a8.4 8.4 0 0 1 9 8.4z" />
    <path d="M8.5 10.5h7M8.5 13.5h4.5" />
  {:else if name === 'volume'}
    <path d="M11 5 6.5 9H3v6h3.5L11 19z" />
    <path d="M15.4 8.6a5 5 0 0 1 0 6.8" />
    <path d="M18.3 5.8a9 9 0 0 1 0 12.4" />
  {:else if name === 'home'}
    <path d="M4 10.5 12 4l8 6.5" />
    <path d="M6.5 9.5V20h11V9.5" />
  {:else if name === 'song'}
    <path d="M9 18V6l10-2v12" />
    <circle cx="6.5" cy="18" r="2.5" />
    <circle cx="16.5" cy="16" r="2.5" />
  {:else if name === 'album'}
    <circle cx="12" cy="12" r="8.5" />
    <circle cx="12" cy="12" r="2" />
  {:else if name === 'artist'}
    <circle cx="12" cy="8" r="3.5" />
    <path d="M5 20c0-3.6 3.1-5.5 7-5.5s7 1.9 7 5.5" />
  {:else if name === 'heart'}
    <path d="M12 20s-7-4.4-7-9a3.9 3.9 0 0 1 7-2.4A3.9 3.9 0 0 1 19 11c0 4.6-7 9-7 9z" />
  {:else if name === 'plus'}
    <path d="M12 6v12M6 12h12" />
  {:else if name === 'settings'}
    <circle cx="12" cy="12" r="3" />
    <path d="M12 3.5v2.2M12 18.3v2.2M4.9 7.8l1.9 1.1M17.2 15.1l1.9 1.1M4.9 16.2l1.9-1.1M17.2 8.9l1.9-1.1" />
  {:else if name === 'radio'}
    <circle cx="12" cy="12" r="2.2" />
    <path d="M7.6 7.6a6.2 6.2 0 0 0 0 8.8M16.4 7.6a6.2 6.2 0 0 1 0 8.8" />
    <path d="M4.6 4.6a10.5 10.5 0 0 0 0 14.8M19.4 4.6a10.5 10.5 0 0 1 0 14.8" />
  {:else if name === 'radar'}
    <circle cx="12" cy="12" r="8.6" />
    <circle cx="12" cy="12" r="4.4" />
    <path d="M12 12l4.8-4.8" />
    <circle cx="12" cy="12" r="1" fill="currentColor" stroke="none" />
  {:else if name === 'queue'}
    <path d="M4 7h11M4 12h11M4 17h7" />
    <path d="M18 10v7l3-2" />
  {/if}
</svg>

<style>
  .ico {
    display: block;
    flex: none;
  }
</style>
