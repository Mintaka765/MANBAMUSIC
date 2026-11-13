<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from './Icon.svelte';
  import type { IconName } from './icons';

  interface MenuItem {
    label: string;
    onselect: () => void;
    icon?: IconName;
    disabled?: boolean;
    danger?: boolean;
    separator?: boolean;
  }

  let {
    x,
    y,
    items,
    onclose,
  }: {
    x: number;
    y: number;
    items: MenuItem[];
    onclose: () => void;
  } = $props();

  let root = $state<HTMLDivElement | null>(null);
  let active = $state(-1);
  let left = $state(x);
  let top = $state(y);

  onMount(() => {
    const node = root;
    if (!node) return;

    // 贴近视口边缘时翻转，别被裁掉
    const rect = node.getBoundingClientRect();
    const margin = 8;
    left = Math.min(x, window.innerWidth - rect.width - margin);
    top = Math.min(y, window.innerHeight - rect.height - margin);
    left = Math.max(margin, left);
    top = Math.max(margin, top);

    function close() {
      onclose();
    }
    function onPointerDown(event: PointerEvent) {
      if (node && !node.contains(event.target as Node)) close();
    }
    function onKeyDown(event: KeyboardEvent) {
      const usable = items.filter((item) => !item.separator && !item.disabled);
      if (event.key === 'Escape') {
        event.preventDefault();
        close();
      } else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
        event.preventDefault();
        if (usable.length === 0) return;
        const step = event.key === 'ArrowDown' ? 1 : -1;
        const current = items.findIndex((item) => item === usable[active]);
        const next = (current + step + usable.length) % usable.length;
        active = items.indexOf(usable[next]);
      } else if (event.key === 'Enter' && active >= 0) {
        event.preventDefault();
        const item = items[active];
        if (item && !item.disabled && !item.separator) {
          close();
          item.onselect();
        }
      }
    }
    function onScroll() {
      close();
    }
    function onResize() {
      close();
    }

    window.addEventListener('pointerdown', onPointerDown, true);
    window.addEventListener('keydown', onKeyDown, true);
    window.addEventListener('scroll', onScroll, true);
    window.addEventListener('resize', onResize);
    window.addEventListener('blur', onResize);
    return () => {
      window.removeEventListener('pointerdown', onPointerDown, true);
      window.removeEventListener('keydown', onKeyDown, true);
      window.removeEventListener('scroll', onScroll, true);
      window.removeEventListener('resize', onResize);
      window.removeEventListener('blur', onResize);
    };
  });

  function run(item: MenuItem) {
    if (item.disabled) return;
    onclose();
    item.onselect();
  }
</script>

<div
  class="menu"
  role="menu"
  tabindex="-1"
  bind:this={root}
  style:left={`${left}px`}
  style:top={`${top}px`}
>
  {#each items as item, index (index)}
    {#if item.separator}
      <div class="sep" role="separator"></div>
    {:else}
      <button
        class="item"
        class:danger={item.danger}
        class:on={index === active}
        role="menuitem"
        disabled={item.disabled}
        onmouseenter={() => (active = index)}
        onclick={() => run(item)}
      >
        <span class="icon">
          {#if item.icon}<Icon name={item.icon} size={14} />{/if}
        </span>
        <span class="label">{item.label}</span>
      </button>
    {/if}
  {/each}
</div>

<style>
  .menu {
    position: fixed;
    z-index: 60;
    min-width: 184px;
    padding: 5px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface-2);
    box-shadow: var(--shadow-2, 0 12px 32px rgba(0, 0, 0, 0.28));
    outline: none;
  }

  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 32px;
    padding: 0 10px;
    border: none;
    border-radius: 7px;
    background: none;
    color: var(--text-dim);
    font-size: 13px;
    text-align: left;
    cursor: pointer;
  }

  .item:hover:not(:disabled),
  .item.on:not(:disabled) {
    background: color-mix(in srgb, var(--text) 8%, transparent);
    color: var(--text);
  }

  .item:disabled {
    opacity: 0.42;
    cursor: default;
  }

  .item.danger:hover:not(:disabled),
  .item.danger.on:not(:disabled) {
    color: var(--accent);
  }

  .icon {
    width: 16px;
    height: 16px;
    display: grid;
    place-items: center;
    color: currentColor;
    flex: none;
  }

  .label {
    flex: 1;
    white-space: nowrap;
  }

  .sep {
    height: 1px;
    margin: 5px 8px;
    background: var(--border);
  }
</style>
