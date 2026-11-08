import { TOKEN_NAMES, type Theme } from './tokens';

/** 把主题写进 CSS 变量。只改 :root 上的变量，组件永远读 var(--x)。 */
export function applyTheme(theme: Theme, root: HTMLElement = document.documentElement) {
  root.dataset.theme = theme.id;
  root.dataset.scheme = theme.colorScheme;
  root.style.colorScheme = theme.colorScheme;

  for (const name of TOKEN_NAMES) {
    root.style.setProperty(`--${name}`, theme.tokens[name]);
  }
}

/** 生成首帧内联脚本要用的扁平对象（也是 localStorage 的缓存格式）。 */
export function serializeTheme(theme: Theme): { id: string; colorScheme: string; tokens: Record<string, string> } {
  return { id: theme.id, colorScheme: theme.colorScheme, tokens: { ...theme.tokens } };
}
