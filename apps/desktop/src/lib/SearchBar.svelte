<script lang="ts">
  let {
    value = $bindable(''),
    loading = false,
    onsubmit,
  }: {
    value?: string;
    loading?: boolean;
    onsubmit: () => void;
  } = $props();
</script>

<form
  class="search"
  onsubmit={(event) => {
    event.preventDefault();
    onsubmit();
  }}
>
  <svg class="icon" viewBox="0 0 24 24" aria-hidden="true">
    <circle cx="11" cy="11" r="7" />
    <path d="m20 20-3.5-3.5" />
  </svg>
  <input
    bind:value
    type="search"
    placeholder="搜索歌曲、歌手、专辑"
    autocomplete="off"
    spellcheck="false"
  />
  <span class="hint" aria-hidden="true">{loading ? '…' : '⌘K'}</span>
</form>

<style>
  /* 下划线式搜索：没有胶囊、没有填充底 */
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 7px 10px;
    border-radius: 7px;
    background: var(--surface-2);
    border: 1px solid transparent;
    transition: border-color 160ms ease;
  }

  .search:focus-within {
    border-color: var(--border-strong);
  }

  .icon {
    width: 15px;
    width: 15px;
    height: 15px;
    flex: none;
    fill: none;
    stroke: var(--text-faint);
    stroke-width: 1.8;
    stroke-linecap: round;
  }

  input {
    flex: 1;
    min-width: 0;
    background: transparent;
    border: none;
    outline: none;
    color: var(--text);
    font-family: var(--font-ui);
    font-size: 13px;
    letter-spacing: 0.01em;
  }

  input::placeholder {
    color: var(--text-faint);
  }

  /* 去掉 Chrome 自带的 search 清除按钮，保持极简 */
  input::-webkit-search-decoration,
  input::-webkit-search-cancel-button {
    -webkit-appearance: none;
  }

  .hint {
    flex: none;
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-faint);
    letter-spacing: 0.06em;
  }
</style>



