<script lang="ts">
  import { PALETTES } from '../theme/presets';
  import { themeStore } from '../theme/store.svelte';
  import {
    deriveTheme,
    schemeOf,
    textContrast,
    type ThemeSeed,
    type Theme,
  } from '../theme/tokens';

  let { onclose }: { onclose: () => void } = $props();

  interface Editing {
    id?: string;
    name: string;
    seed: ThemeSeed;
  }

  let editing = $state<Editing | null>(null);

  function startNew() {
    editing = {
      name: '我的主题',
      seed: { ...themeStore.seedFor(themeStore.currentId) },
    };
  }

  function startEdit(theme: Theme) {
    editing = {
      id: theme.id,
      name: theme.name,
      seed: { ...themeStore.seedFor(theme.id) },
    };
  }

  function setColor(role: keyof ThemeSeed, color: string) {
    if (!editing) return;
    editing = { ...editing, seed: { ...editing.seed, [role]: color } };
  }

  function save() {
    if (!editing) return;
    themeStore.saveCustom(editing.name, editing.seed, editing.id);
    editing = null;
  }

  function remove(id: string) {
    themeStore.removeCustom(id);
    if (editing?.id === id) editing = null;
  }

  const previewTokens = $derived(editing ? deriveTheme(editing.seed) : null);
  const previewScheme = $derived(editing ? schemeOf(editing.seed) : 'dark');
  const previewStyle = $derived(
    previewTokens
      ? Object.entries(previewTokens)
          .map(([key, value]) => `--${key}: ${value}`)
          .join('; ')
      : '',
  );
  const contrastValue = $derived(
    editing
      ? textContrast({
          id: 'preview',
          name: '',
          colorScheme: previewScheme,
          tokens: previewTokens!,
          builtin: false,
        })
      : 0,
  );

  const roles: Array<keyof ThemeSeed> = ['bg', 'surface', 'text', 'accent'];
</script>

<div class="backdrop" role="presentation" onclick={onclose}>
  <div class="panel" role="dialog" aria-modal="true" onclick={(event) => event.stopPropagation()}>
    <header>
      <h2>主题</h2>
      <button class="ghost" onclick={onclose}>关闭</button>
    </header>

    {#if editing}
      <div class="editor">
        <label class="field">
          <span>名称</span>
          <input bind:value={editing.name} maxlength="20" />
        </label>

        {#each roles as role (role)}
          <div class="palette">
            <span class="role">{PALETTES[role].name}</span>
            <div class="swatches">
              {#each PALETTES[role].colors as color (color)}
                <button
                  class="swatch"
                  class:active={editing.seed[role] === color}
                  style:background={color}
                  title={color}
                  aria-label={`${PALETTES[role].name} ${color}`}
                  onclick={() => setColor(role, color)}
                ></button>
              {/each}
            </div>
          </div>
        {/each}

        <div class="preview" style={previewStyle} data-scheme={previewScheme}>
          <div class="p-cover"></div>
          <div class="p-body">
            <p class="p-title">晴天</p>
            <p class="p-meta">周杰伦 · 叶惠美</p>
            <div class="p-bar"><div class="p-fill"></div></div>
            <p class="p-lyric">故事的小黄花</p>
          </div>
        </div>

        <p class="hint" class:bad={contrastValue < 4.5}>
          正文对比度 {contrastValue.toFixed(1)}:1
          {contrastValue < 4.5 ? '（低于 4.5，文字可能看不清）' : '（达标）'}
        </p>

        <div class="actions">
          <button class="primary" onclick={save}>保存</button>
          <button class="ghost" onclick={() => (editing = null)}>取消</button>
        </div>
      </div>
    {:else}
      <div class="themes">
        {#each themeStore.themes as theme (theme.id)}
          <div class="card" class:active={themeStore.currentId === theme.id}>
            <button
              class="pick"
              data-theme-id={theme.id}
              onclick={() => themeStore.select(theme.id)}
              title={theme.name}
            >
              <span class="swatches-mini">
                <i style:background={theme.tokens.bg}></i>
                <i style:background={theme.tokens['surface-2']}></i>
                <i style:background={theme.tokens.accent}></i>
              </span>
              <span class="name">{theme.name}</span>
            </button>
            <div class="card-actions">
              <button class="mini" onclick={() => startEdit(theme)}>编辑</button>
              {#if !theme.builtin}
                <button class="mini danger" onclick={() => remove(theme.id)}>删除</button>
              {/if}
            </div>
          </div>
        {/each}
      </div>

      <div class="actions">
        <button class="primary" onclick={startNew}>新建主题</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: grid;
    place-items: center;
    z-index: 30;
  }

  .panel {
    width: 560px;
    max-height: 82vh;
    overflow-y: auto;
    padding: 20px 22px;
    border-radius: var(--radius-lg);
    background: var(--surface-1);
    border: 1px solid var(--border);
    box-shadow: var(--shadow-2);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 14px;
  }

  h2 {
    margin: 0;
    font-size: 15px;
  }

  .themes {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 10px;
  }

  .card {
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    overflow: hidden;
    background: var(--surface-2);
  }

  .card.active {
    border-color: var(--accent);
  }

  .pick {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px;
    border: none;
    background: transparent;
    color: var(--text);
    cursor: pointer;
    text-align: left;
  }

  .swatches-mini {
    display: flex;
    gap: 4px;
  }

  .swatches-mini i {
    width: 22px;
    height: 22px;
    border-radius: 6px;
    border: 1px solid var(--border);
  }

  .name {
    font-size: 12px;
  }

  .card-actions {
    display: flex;
    gap: 6px;
    padding: 0 10px 10px;
  }

  .mini {
    border: 1px solid var(--border);
    background: transparent;
    color: var(--text-dim);
    border-radius: 6px;
    padding: 2px 8px;
    font-size: 11px;
    cursor: pointer;
  }

  .mini.danger {
    color: var(--danger);
    border-color: color-mix(in srgb, var(--danger) 40%, transparent);
  }

  .editor {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .field {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 12px;
    color: var(--text-dim);
  }

  .field input {
    flex: 1;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text);
    padding: 6px 10px;
    font-size: 13px;
  }

  .palette {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .role {
    width: 36px;
    font-size: 12px;
    color: var(--text-dim);
  }

  .swatches {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .swatch {
    width: 24px;
    height: 24px;
    border-radius: 6px;
    border: 1px solid var(--border);
    cursor: pointer;
  }

  .swatch.active {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .preview {
    display: flex;
    gap: 12px;
    padding: 12px;
    border-radius: var(--radius-md);
    background: var(--surface-1);
    border: 1px solid var(--border);
  }

  .p-cover {
    width: 48px;
    height: 48px;
    border-radius: 8px;
    background: linear-gradient(135deg, var(--accent), color-mix(in srgb, var(--accent) 40%, var(--bg)));
  }

  .p-body {
    flex: 1;
    min-width: 0;
  }

  .p-title {
    margin: 0;
    font-size: 13px;
    color: var(--text);
  }

  .p-meta {
    margin: 2px 0 6px;
    font-size: 11px;
    color: var(--text-dim);
  }

  .p-bar {
    height: 3px;
    border-radius: 999px;
    background: var(--surface-2);
    overflow: hidden;
  }

  .p-fill {
    width: 40%;
    height: 100%;
    background: var(--accent);
  }

  .p-lyric {
    margin: 8px 0 0;
    font-size: 11px;
    color: var(--accent);
  }

  .hint {
    margin: 0;
    font-size: 11px;
    color: var(--text-dim);
  }

  .hint.bad {
    color: var(--warning);
  }

  .actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
    margin-top: 14px;
  }

  .primary {
    border: none;
    border-radius: var(--radius-sm);
    background: var(--accent);
    color: var(--accent-contrast);
    padding: 7px 16px;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }

  .ghost {
    border: 1px solid var(--border);
    background: transparent;
    color: var(--text);
    border-radius: var(--radius-sm);
    padding: 6px 14px;
    font-size: 12px;
    cursor: pointer;
  }
</style>
