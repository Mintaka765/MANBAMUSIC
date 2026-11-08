/** 颜色工具：全部纯函数，主题派生都靠它们。 */

export type Rgb = { r: number; g: number; b: number };
export type Hsl = { h: number; s: number; l: number };

export function parseHex(input: string): Rgb {
  let hex = input.trim().replace('#', '');
  if (hex.length === 3) {
    hex = hex
      .split('')
      .map((ch) => ch + ch)
      .join('');
  }
  const value = Number.parseInt(hex, 16);
  if (Number.isNaN(value) || hex.length !== 6) {
    return { r: 0, g: 0, b: 0 };
  }
  return {
    r: (value >> 16) & 0xff,
    g: (value >> 8) & 0xff,
    b: value & 0xff,
  };
}

export function toHex({ r, g, b }: Rgb): string {
  const clamp = (n: number) => Math.max(0, Math.min(255, Math.round(n)));
  return `#${[r, g, b].map((n) => clamp(n).toString(16).padStart(2, '0')).join('')}`;
}

/** t=0 返回 a，t=1 返回 b。 */
export function mix(a: string, b: string, t: number): string {
  const ca = parseHex(a);
  const cb = parseHex(b);
  return toHex({
    r: ca.r + (cb.r - ca.r) * t,
    g: ca.g + (cb.g - ca.g) * t,
    b: ca.b + (cb.b - ca.b) * t,
  });
}

export function rgbToHsl({ r, g, b }: Rgb): Hsl {
  const rn = r / 255;
  const gn = g / 255;
  const bn = b / 255;
  const max = Math.max(rn, gn, bn);
  const min = Math.min(rn, gn, bn);
  const delta = max - min;
  const l = (max + min) / 2;

  if (delta === 0) {
    return { h: 0, s: 0, l };
  }

  const s = delta / (1 - Math.abs(2 * l - 1));
  let h: number;
  if (max === rn) h = ((gn - bn) / delta) % 6;
  else if (max === gn) h = (bn - rn) / delta + 2;
  else h = (rn - gn) / delta + 4;

  h *= 60;
  if (h < 0) h += 360;
  return { h, s, l };
}

export function hslToHex({ h, s, l }: Hsl): string {
  const c = (1 - Math.abs(2 * l - 1)) * s;
  const hp = (h % 360) / 60;
  const x = c * (1 - Math.abs((hp % 2) - 1));
  let rgb = { r: 0, g: 0, b: 0 };
  if (hp < 1) rgb = { r: c, g: x, b: 0 };
  else if (hp < 2) rgb = { r: x, g: c, b: 0 };
  else if (hp < 3) rgb = { r: 0, g: c, b: x };
  else if (hp < 4) rgb = { r: 0, g: x, b: c };
  else if (hp < 5) rgb = { r: x, g: 0, b: c };
  else rgb = { r: c, g: 0, b: x };
  const m = l - c / 2;
  return toHex({
    r: (rgb.r + m) * 255,
    g: (rgb.g + m) * 255,
    b: (rgb.b + m) * 255,
  });
}

function channel(n: number): number {
  const v = n / 255;
  return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
}

/** WCAG 相对亮度。 */
export function luminance(hex: string): number {
  const { r, g, b } = parseHex(hex);
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

/** WCAG 对比度，1~21。 */
export function contrast(a: string, b: string): number {
  const la = luminance(a);
  const lb = luminance(b);
  const [hi, lo] = la > lb ? [la, lb] : [lb, la];
  return (hi + 0.05) / (lo + 0.05);
}

/** 在浅色/深色文字里挑一个和背景对比度更高的。 */
export function bestTextOn(background: string): string {
  return contrast(background, '#ffffff') >= contrast(background, '#0b0d10')
    ? '#ffffff'
    : '#0b0d10';
}

/**
 * 把颜色往"离背景更远"的方向调整，直到达到最小对比度。
 * 用于 accent 这种既要好看又必须看得清的色。
 */
export function ensureContrast(color: string, background: string, minimum = 4.5): string {
  if (contrast(color, background) >= minimum) {
    return color;
  }

  const hsl = rgbToHsl(parseHex(color));
  const lighter = luminance(background) < 0.5;
  let best = color;

  for (let step = 1; step <= 20; step++) {
    const delta = step * 0.03;
    const candidate = hslToHex({
      h: hsl.h,
      s: Math.min(1, hsl.s + delta * 0.2),
      l: lighter ? Math.min(0.92, hsl.l + delta) : Math.max(0.08, hsl.l - delta),
    });
    if (contrast(candidate, background) >= minimum) {
      best = candidate;
      break;
    }
    best = candidate;
  }
  return best;
}
