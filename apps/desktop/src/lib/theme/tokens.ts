import { bestTextOn, contrast, ensureContrast, luminance, mix } from './color';

/** CSS 变量名（不含 `--`）。首帧脚本和 applyTheme 都直接用这些键。 */
export const TOKEN_NAMES = [
  'bg',
  'surface-1',
  'surface-2',
  'surface-3',
  'text',
  'text-dim',
  'text-faint',
  'accent',
  'accent-contrast',
  'danger',
  'warning',
  'border',
  'border-strong',
  'radius-sm',
  'radius-md',
  'radius-lg',
  'shadow-1',
  'shadow-2',
  'blur',
  'font-ui',
  'font-display',
  'font-mono',
  'font-lyric',
  'lyric-text',
  'lyric-shadow',
] as const;

export type TokenName = (typeof TOKEN_NAMES)[number];
export type ThemeTokens = Record<TokenName, string>;
export type ColorScheme = 'dark' | 'light';

/** 主题只需要 4 个种子色，其余由规则派生。 */
export interface ThemeSeed {
  bg: string;
  surface: string;
  text: string;
  accent: string;
}

export interface Theme {
  id: string;
  name: string;
  colorScheme: ColorScheme;
  tokens: ThemeTokens;
  /** 内置主题不允许删除 */
  builtin: boolean;
}

export function schemeOf(seed: ThemeSeed): ColorScheme {
  return luminance(seed.bg) > 0.35 ? 'light' : 'dark';
}

/**
 * 从 4 个种子色派生出完整 token 表。
 * 这样编辑器只要管 4 个颜色，其余 19 个永远保持一致。
 */
export function deriveTheme(seed: ThemeSeed, scheme = schemeOf(seed)): ThemeTokens {
  const dark = scheme === 'dark';
  const accent = ensureContrast(seed.accent, seed.bg, 3.2);

  return {
    bg: seed.bg,
    'surface-1': mix(seed.bg, seed.surface, 0.5),
    'surface-2': mix(seed.bg, seed.surface, dark ? 0.85 : 0.7),
    'surface-3': mix(seed.surface, dark ? '#ffffff' : '#000000', 0.06),
    text: seed.text,
    'text-dim': mix(seed.text, seed.bg, 0.42),
    'text-faint': mix(seed.text, seed.bg, 0.66),
    accent,
    'accent-contrast': bestTextOn(accent),
    danger: dark ? '#d9534a' : '#b3261e',
    warning: dark ? '#c9a227' : '#8a6d12',
    // 发丝线用半透明，深浅两种模式下都能自然叠在背景上
    border: `color-mix(in srgb, ${seed.text} ${dark ? 8 : 12}%, transparent)`,
    'border-strong': `color-mix(in srgb, ${seed.text} ${dark ? 16 : 22}%, transparent)`,
    'radius-sm': '4px',
    'radius-md': '6px',
    'radius-lg': '10px',
    'shadow-1': dark ? '0 1px 2px rgba(0, 0, 0, 0.3)' : '0 1px 2px rgba(26, 23, 20, 0.06)',
    'shadow-2': dark ? '0 24px 60px rgba(0, 0, 0, 0.55)' : '0 24px 60px rgba(26, 23, 20, 0.12)',
    blur: '18px',
    // 和 mygo-music 一样：不打包字体，直接用系统原生字体
    'font-ui':
      "'Segoe UI Variable Text', 'Segoe UI', 'PingFang SC', 'Microsoft YaHei', system-ui, -apple-system, sans-serif",
    'font-display':
      "'Segoe UI Variable Text', 'Segoe UI', 'PingFang SC', 'Microsoft YaHei', system-ui, sans-serif",
    'font-mono':
      "'Segoe UI', 'PingFang SC', 'Microsoft YaHei', system-ui, sans-serif",
    'font-lyric':
      "'Segoe UI', 'PingFang SC', 'Microsoft YaHei', system-ui, sans-serif",
    // 桌面歌词窗永远压在别的窗口上，固定浅色文字 + 强阴影，跟主题深浅无关
    'lyric-text': '#ffffff',
    'lyric-shadow': '0 2px 12px rgba(0, 0, 0, 0.9), 0 0 2px rgba(0, 0, 0, 0.95)',
  };
}

export function themeFromSeed(
  id: string,
  name: string,
  seed: ThemeSeed,
  builtin = false,
): Theme {
  const scheme = schemeOf(seed);
  return { id, name, colorScheme: scheme, tokens: deriveTheme(seed, scheme), builtin };
}

/** 判断主题的正文对比度是否达标（编辑器用来提示）。 */
export function textContrast(theme: Theme): number {
  return contrast(theme.tokens.text, theme.tokens.bg);
}


