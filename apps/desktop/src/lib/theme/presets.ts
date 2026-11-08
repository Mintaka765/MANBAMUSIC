import type { ThemeSeed } from './tokens';

export interface PresetSeed {
  id: string;
  name: string;
  seed: ThemeSeed;
}

/** 内置主题：只存 4 个种子色，其余自动派生。 */
export const PRESET_SEEDS: PresetSeed[] = [
  {
    id: 'ink',
    name: '午夜黑胶',
    seed: { bg: '#0c0b0a', surface: '#141210', text: '#f2ede4', accent: '#e2703a' },
  },
  {
    id: 'paper',
    name: '纸感',
    seed: { bg: '#f4f0e8', surface: '#faf7f1', text: '#1a1714', accent: '#c8562a' },
  },
  {
    id: 'graphite',
    name: '石墨',
    seed: { bg: '#141414', surface: '#242424', text: '#ededed', accent: '#f2b544' },
  },
  {
    id: 'ocean',
    name: '深海',
    seed: { bg: '#071018', surface: '#12293c', text: '#dbe7f3', accent: '#4aa3ff' },
  },
  {
    id: 'sakura',
    name: '夜樱',
    seed: { bg: '#140b10', surface: '#2a1620', text: '#f3e7ec', accent: '#ff7aa2' },
  },

];

/** 编辑器用的色板：每个角色给一批预设色，不做自由取色。 */
export const PALETTES: Record<keyof ThemeSeed, { name: string; colors: string[] }> = {
  bg: {
    name: '背景',
    colors: [
      '#0c0b0a',
      '#101014',
      '#141414',
      '#071018',
      '#140b10',
      '#000000',
      '#f4f0e8',
      '#f7f7f8',
      '#faf9f6',
      '#f2f4f7',
    ],
  },
  surface: {
    name: '表面',
    colors: [
      '#141210',
      '#1c1917',
      '#242424',
      '#12293c',
      '#2a1620',
      '#ffffff',
      '#faf7f1',
      '#eef1f5',
      '#efe8dc',
    ],
  },
  text: {
    name: '文字',
    colors: [
      '#f2ede4',
      '#e8eaed',
      '#f5f5f5',
      '#dbe7f3',
      '#ffffff',
      '#1a1714',
      '#17181a',
      '#2b2620',
      '#0f172a',
    ],
  },
  accent: {
    name: '强调',
    colors: [
      '#e2703a',
      '#c8562a',
      '#c9a227',
      '#4dd4ac',
      '#4aa3ff',
      '#f2b544',
      '#ff7aa2',
      '#a78bfa',
      '#7ed957',
      '#e5484d',
      '#22d3ee',
      '#ffe14d',
    ],
  },
};

