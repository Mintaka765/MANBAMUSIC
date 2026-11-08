import { invoke } from '@tauri-apps/api/core';
import type {
  AppSettings,
  LyricTrack,
  PlayerSnapshot,
  Playlist,
  QrStatus,
  QueueState,
  Track,
  UserProfile,
} from './types';

/**
 * 前端访问后端的唯一入口。
 *
 * 在 Tauri 窗口里走 `invoke`，在浏览器里 `npm run dev` 时退回 mock，
 * 这样调样式不用每次起 Rust。
 */

const MOCK: Track[] = [
  {
    id: 186016,
    name: '晴天',
    artists: ['周杰伦'],
    album: '叶惠美',
    coverUrl: null,
    durationMs: 269_000,
    fee: 1,
  },
  {
    id: 26259733,
    name: 'Luv (Sic) Grand Finale Part 6',
    artists: ['Nujabes'],
    album: 'Luv(sic) Hexalogy',
    coverUrl: null,
    durationMs: 301_000,
    fee: 0,
  },
  {
    id: 1974443814,
    name: '我记得',
    artists: ['赵雷'],
    album: '署前街少年',
    coverUrl: null,
    durationMs: 335_000,
    fee: 1,
  },
];

const IDLE: PlayerSnapshot = {
  state: 'idle',
  positionMs: 0,
  durationMs: 0,
  volume: 0.8,
  error: null,
  trackId: null,
  trialSecs: null,
};

export const isDesktop: boolean =
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

/**
 * 网易云 CDN 支持按尺寸裁剪：?param=80y80。
 * 不裁剪的话一张 2000×2000 的封面解码后要占 16MB 显存/内存。
 */
export function coverUrl(url: string | null | undefined, size: number): string | null {
  if (!url) return null;
  const secure = url.replace(/^http:\/\//, 'https://');
  const separator = secure.includes('?') ? '&' : '?';
  return `${secure}${separator}param=${size}y${size}`;
}

export async function searchTracks(keyword: string, limit = 20): Promise<Track[]> {
  const trimmed = keyword.trim();
  if (!trimmed) return [];

  if (isDesktop) {
    return invoke<Track[]>('search_tracks', { keyword: trimmed, limit });
  }

  // 浏览器预览用的假数据，延迟一下让 loading 状态可见
  await new Promise((resolve) => setTimeout(resolve, 220));
  return MOCK.filter(
    (track) =>
      track.name.includes(trimmed) || track.artists.some((artist) => artist.includes(trimmed)),
  ).slice(0, limit);
}

export async function playTrack(id: number): Promise<void> {
  if (isDesktop) await invoke('play_track', { id });
}

export async function playQueue(tracks: Track[], start: number): Promise<void> {
  if (isDesktop) await invoke('play_queue', { tracks, start });
}

export async function nextTrack(): Promise<void> {
  if (isDesktop) await invoke('next_track');
}

export async function previousTrack(): Promise<void> {
  if (isDesktop) await invoke('previous_track');
}

export async function getLyric(id: number): Promise<LyricTrack | null> {
  if (!isDesktop) return null;
  return invoke<LyricTrack>('get_lyric', { id });
}

export async function pausePlayer(): Promise<void> {
  if (isDesktop) await invoke('pause_player');
}

export async function resumePlayer(): Promise<void> {
  if (isDesktop) await invoke('resume_player');
}

export async function stopPlayer(): Promise<void> {
  if (isDesktop) await invoke('stop_player');
}

export async function seekPlayer(positionMs: number): Promise<void> {
  if (isDesktop) await invoke('seek_player', { positionMs });
}

/** 0.0 ~ 1.0 */
export async function setVolume(volume: number): Promise<void> {
  if (isDesktop) await invoke('set_volume', { volume });
}

/** 喜欢 / 取消喜欢一首歌，返回操作后的状态。 */
export async function likeSong(id: number, like: boolean): Promise<boolean> {
  if (!isDesktop) return like;
  return invoke<boolean>('like_song', { id, like });
}

/** 批量查询已喜欢的曲目 id。 */
export async function likedSongs(ids: number[]): Promise<number[]> {
  if (!isDesktop || ids.length === 0) return [];
  return invoke<number[]>('liked_songs', { ids });
}

/** 待播清单（按实际播放顺序）。 */
export interface QueueItems {
  tracks: Track[];
  position: number;
}

export async function queueItems(): Promise<QueueItems> {
  if (!isDesktop) return { tracks: [], position: 0 };
  return invoke<QueueItems>('queue_items');
}

/** 从待播清单的第 index 项开始播放。 */
export async function jumpTo(index: number): Promise<void> {
  if (isDesktop) await invoke('jump_to', { index });
}
export async function getSettings(): Promise<AppSettings> {
  if (!isDesktop) return { level: 'exhigh', closeToTray: false };
  return invoke<AppSettings>('get_settings');
}

export async function saveSettings(settings: AppSettings): Promise<AppSettings> {
  if (!isDesktop) return settings;
  return invoke<AppSettings>('save_settings', { settings });
}
/** 每日推荐（未登录时后端会返回空数组或报错）。 */
export async function dailySongs(): Promise<Track[]> {
  if (!isDesktop) return [];
  return invoke<Track[]>('daily_songs');
}
/** 开始漫游（私人 FM），失败抛错。 */
export async function playRadio(): Promise<void> {
  if (isDesktop) await invoke('play_radio');
}

/** 私人雷达歌单曲目。 */
export async function radarTracks(): Promise<Track[]> {
  if (!isDesktop) return [];
  return invoke<Track[]>('radar_tracks');
}
export async function playerSnapshot(): Promise<PlayerSnapshot> {
  if (!isDesktop) return IDLE;
  return invoke<PlayerSnapshot>('player_snapshot');
}

export async function queueState(): Promise<QueueState | null> {
  if (!isDesktop) return null;
  return invoke<QueueState>('queue_state');
}

export async function setRepeat(mode: QueueState['repeat']): Promise<QueueState | null> {
  if (!isDesktop) return null;
  return invoke<QueueState>('set_repeat', { mode });
}

export async function toggleShuffle(enabled: boolean): Promise<QueueState | null> {
  if (!isDesktop) return null;
  return invoke<QueueState>('toggle_shuffle', { enabled });
}

export async function toggleLyrics(enabled: boolean): Promise<void> {
  if (isDesktop) await invoke('toggle_lyrics', { enabled });
}

export async function setLyricsClickThrough(ignore: boolean): Promise<void> {
  if (isDesktop) await invoke('set_lyrics_click_through', { ignore });
}

// ---- 登录 ----

export async function loginQrStart(): Promise<string> {
  return invoke<string>('login_qr_start');
}

export async function loginQrPoll(key: string): Promise<QrStatus> {
  return invoke<QrStatus>('login_qr_poll', { key });
}

export async function myProfile(): Promise<UserProfile | null> {
  if (!isDesktop) return null;
  return invoke<UserProfile | null>('my_profile');
}

export async function logout(): Promise<void> {
  if (isDesktop) await invoke('logout');
}

export async function myPlaylists(): Promise<Playlist[]> {
  if (!isDesktop) return [];
  return invoke<Playlist[]>('my_playlists');
}

export async function playlistTracks(id: number): Promise<Track[]> {
  if (!isDesktop) return [];
  return invoke<Track[]>('playlist_tracks', { id });
}





