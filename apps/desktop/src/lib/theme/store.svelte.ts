import { applyTheme, serializeTheme } from './apply';
import { PRESET_SEEDS } from './presets';
import { themeFromSeed, type Theme, type ThemeSeed } from './tokens';

const CUSTOM_KEY = 'mbm-themes-custom';
const CURRENT_KEY = 'mbm-theme';

interface StoredCustom {
  id: string;
  name: string;
  seed: ThemeSeed;
}

function builtinThemes(): Theme[] {
  return PRESET_SEEDS.map((preset) => themeFromSeed(preset.id, preset.name, preset.seed, true));
}

function builtinSeeds(): Record<string, ThemeSeed> {
  return Object.fromEntries(PRESET_SEEDS.map((preset) => [preset.id, preset.seed]));
}

class ThemeStore {
  themes = $state<Theme[]>(builtinThemes());
  currentId = $state<string>(PRESET_SEEDS[0].id);
  seeds = $state<Record<string, ThemeSeed>>(builtinSeeds());

  get current(): Theme {
    return this.themes.find((theme) => theme.id === this.currentId) ?? this.themes[0];
  }

  seedFor(id: string): ThemeSeed {
    return this.seeds[id] ?? PRESET_SEEDS[0].seed;
  }

  /** 启动时调用：恢复自定义主题、恢复上次选择、应用到 DOM。 */
  init() {
    try {
      const raw = localStorage.getItem(CUSTOM_KEY);
      if (raw) {
        const stored = JSON.parse(raw) as StoredCustom[];
        for (const item of stored) {
          if (!item?.seed) continue;
          this.themes = [...this.themes, themeFromSeed(item.id, item.name, item.seed)];
          this.seeds = { ...this.seeds, [item.id]: item.seed };
        }
      }
    } catch {
      // 存的东西坏了就当没有
    }

    try {
      const raw = localStorage.getItem(CURRENT_KEY);
      if (raw) {
        const parsed = JSON.parse(raw) as { id?: string };
        // 「极简白」已并入「纸感」，旧记录迁移过去
        const savedId = parsed.id === 'minimal' ? 'paper' : parsed.id;
        if (savedId && this.themes.some((theme) => theme.id === savedId)) {
          this.currentId = savedId;
        }
      }
    } catch {
      // 同上
    }

    applyTheme(this.current);
  }

  select(id: string) {
    if (!this.themes.some((theme) => theme.id === id)) return;
    this.currentId = id;
    applyTheme(this.current);
    try {
      localStorage.setItem(CURRENT_KEY, JSON.stringify(serializeTheme(this.current)));
    } catch {
      // 忽略存储失败
    }
  }

  saveCustom(name: string, seed: ThemeSeed, existingId?: string): Theme {
    const id = existingId ?? `custom-${Date.now().toString(36)}`;
    const theme = themeFromSeed(id, name.trim() || '自定义主题', seed);
    const exists = this.themes.some((item) => item.id === id);
    this.themes = exists
      ? this.themes.map((item) => (item.id === id ? theme : item))
      : [...this.themes, theme];
    this.seeds = { ...this.seeds, [id]: seed };
    this.persistCustom();
    this.select(id);
    return theme;
  }

  removeCustom(id: string) {
    const theme = this.themes.find((item) => item.id === id);
    if (!theme || theme.builtin) return;

    this.themes = this.themes.filter((item) => item.id !== id);
    const { [id]: _removed, ...rest } = this.seeds;
    this.seeds = rest;
    this.persistCustom();

    if (this.currentId === id) {
      this.select(this.themes[0].id);
    }
  }

  private persistCustom() {
    const stored: StoredCustom[] = this.themes
      .filter((theme) => !theme.builtin)
      .map((theme) => ({ id: theme.id, name: theme.name, seed: this.seedFor(theme.id) }));
    try {
      localStorage.setItem(CUSTOM_KEY, JSON.stringify(stored));
    } catch {
      // 忽略存储失败
    }
  }
}

export const themeStore = new ThemeStore();

