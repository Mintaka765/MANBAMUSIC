<script lang="ts">
  import { onMount } from 'svelte';
  import { getLyric, playerSnapshot } from './lib/api';
  import type { LyricLine } from './lib/types';

  let lines = $state<LyricLine[]>([]);
  let positionMs = $state(0);
  let trackId = $state<number | null>(null);

  onMount(() => {
    const timer = setInterval(async () => {
      try {
        const snapshot = await playerSnapshot();
        positionMs = snapshot.positionMs;
        if (snapshot.trackId && snapshot.trackId !== trackId) {
          trackId = snapshot.trackId;
          const lyric = await getLyric(snapshot.trackId);
          lines = lyric?.lines ?? [];
        }
      } catch {
        // 主窗口没起来时静默
      }
    }, 300);
    return () => clearInterval(timer);
  });

  const activeIndex = $derived.by(() => {
    let low = 0;
    let high = lines.length - 1;
    let answer = -1;
    while (low <= high) {
      const mid = (low + high) >> 1;
      if (lines[mid].startMs <= positionMs) {
        answer = mid;
        low = mid + 1;
      } else {
        high = mid - 1;
      }
    }
    return answer;
  });

  const current = $derived(activeIndex >= 0 ? lines[activeIndex] : null);
  const upcoming = $derived(
    activeIndex + 1 < lines.length ? lines[activeIndex + 1] : null,
  );
</script>

<div class="stage">
  {#if current}
    <p class="current">{current.text}</p>
    {#if current.translation}
      <p class="translation">{current.translation}</p>
    {/if}
    {#if upcoming}
      <p class="next">{upcoming.text}</p>
    {/if}
  {:else}
    <p class="idle">Manbamusic · 等待播放</p>
  {/if}
</div>

<style>
  .stage {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100vh;
    padding: 0 24px;
    text-align: center;
    gap: 10px;
    /* 桌面歌词窗：整窗可拖动 */
    -webkit-app-region: drag;
    user-select: none;
  }

  p {
    margin: 0;
    text-shadow: var(--lyric-shadow);
  }

  .current {
    font-size: 30px;
    font-weight: 700;
    color: var(--lyric-text);
    letter-spacing: 0.5px;
  }

  .translation {
    font-size: 15px;
    color: color-mix(in srgb, var(--lyric-text) 78%, transparent);
  }

  .next {
    font-size: 15px;
    color: color-mix(in srgb, var(--lyric-text) 40%, transparent);
    margin-top: 4px;
  }

  .idle {
    font-size: 15px;
    color: color-mix(in srgb, var(--lyric-text) 50%, transparent);
  }
</style>
