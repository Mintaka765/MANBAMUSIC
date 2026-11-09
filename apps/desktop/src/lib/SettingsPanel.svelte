<script lang="ts">
  import { getSettings, saveSettings } from './api';
  import type { AppSettings, AudioLevel } from './types';

  let {
    open,
    onclose,
    ontheme,
  }: {
    open: boolean;
    onclose: () => void;
    ontheme: () => void;
  } = $props();

  const LEVELS: { value: AudioLevel; label: string; hint: string }[] = [
    { value: 'standard', label: '标准', hint: '128kbps，省流量' },
    { value: 'higher', label: '较高', hint: '192kbps' },
    { value: 'exhigh', label: '极高', hint: '320kbps，默认' },
    { value: 'lossless', label: '无损', hint: 'FLAC，需要会员' },
  ];

  let settings = $state<AppSettings>({ level: 'exhigh', closeToTray: false, localFolders: [] });
  let saving = $state(false);

  $effect(() => {
    if (!open) return;
    void getSettings()
      .then((value) => (settings = value))
      .catch(() => {});
  });

  async function update(patch: Partial<AppSettings>) {
    settings = { ...settings, ...patch };
    saving = true;
    try {
      settings = await saveSettings(settings);
    } catch {
      // 保存失败保持界面值，下次打开会读回服务端的
    } finally {
      saving = false;
    }
  }
</script>

{#if open}
  <div
    class="backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) onclose();
    }}
  >
    <div class="card" role="dialog" aria-label="设置">
      <header>
        <h2>设置</h2>
        <span class="state">{saving ? '保存中…' : ''}</span>
        <button class="close" aria-label="关闭设置" onclick={onclose}>✕</button>
      </header>

      <section>
        <p class="label">在线播放音质</p>
        <div class="options">
          {#each LEVELS as item (item.value)}
            <button
              class="option"
              class:active={settings.level === item.value}
              onclick={() => update({ level: item.value })}
            >
              <span class="name">{item.label}</span>
              <span class="hint">{item.hint}</span>
            </button>
          {/each}
        </div>
        <p class="note">无损需要会员；拿不到所选音质时会自动回落到标准音质。</p>
      </section>

      <section>
        <p class="label">关闭窗口时</p>
        <div class="options">
          <button
            class="option"
            class:active={!settings.closeToTray}
            onclick={() => update({ closeToTray: false })}
          >
            <span class="name">退出程序</span>
            <span class="hint">停止播放并关闭</span>
          </button>
          <button
            class="option"
            class:active={settings.closeToTray}
            onclick={() => update({ closeToTray: true })}
          >
            <span class="name">最小化到托盘</span>
            <span class="hint">继续播放，点托盘图标回来</span>
          </button>
        </div>
      </section>

      <section>
        <p class="label">外观</p>
        <button class="link" onclick={ontheme}>打开主题选择器 →</button>
      </section>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: grid;
    place-items: center;
    padding: 24px;
    background: color-mix(in srgb, #000 42%, transparent);
    animation: fade 140ms ease;
  }

  @keyframes fade {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  .card {
    width: min(460px, 100%);
    max-height: 80vh;
    overflow: hidden auto;
    padding: 18px 20px 20px;
    border-radius: 12px;
    background: var(--surface-1);
    box-shadow: var(--shadow-2);
    animation: rise 160ms cubic-bezier(0.2, 0.7, 0.3, 1);
  }

  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(6px);
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
    margin-bottom: 6px;
  }

  h2 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    color: var(--text);
  }

  .state {
    font-size: 11px;
    color: var(--text-faint);
  }

  .close {
    margin-left: auto;
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: var(--text-faint);
    cursor: pointer;
    transition:
      background 120ms ease,
      color 120ms ease;
  }

  .close:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  section {
    padding: 12px 0;
  }

  section + section {
    border-top: 1px solid var(--border);
  }

  .label {
    margin: 0 0 8px;
    font-size: 11px;
    letter-spacing: 0.08em;
    color: var(--text-faint);
  }

  .options {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
  }

  .option {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 9px 11px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: transparent;
    color: var(--text-dim);
    text-align: left;
    cursor: pointer;
    transition:
      border-color 140ms ease,
      background 140ms ease,
      color 140ms ease;
  }

  .option:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .option.active {
    border-color: color-mix(in srgb, var(--accent) 55%, transparent);
    background: color-mix(in srgb, var(--accent) 8%, transparent);
    color: var(--text);
  }

  .name {
    font-size: 13px;
  }

  .hint {
    font-size: 11px;
    color: var(--text-faint);
  }

  .note {
    margin: 8px 0 0;
    font-size: 11px;
    color: var(--text-faint);
    line-height: 1.5;
  }

  .link {
    border: none;
    background: transparent;
    padding: 0;
    color: var(--accent);
    font-size: 13px;
    cursor: pointer;
  }

  .link:hover {
    text-decoration: underline;
  }
</style>
