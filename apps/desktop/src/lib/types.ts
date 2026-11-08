/** 与 Rust 侧 `ncm_api::Song` 对齐的前端视图模型。 */
export interface Track {
  id: number;
  name: string;
  artists: string[];
  album: string;
  coverUrl: string | null;
  /** 毫秒 */
  durationMs: number;
  /** 0=免费 1=VIP 8=低音质免费 */
  fee: number;
}

/** 与 Rust 侧 `PlayerSnapshotDto` 对齐。 */
export interface PlayerSnapshot {
  state: 'idle' | 'loading' | 'playing' | 'paused' | 'error';
  positionMs: number;
  durationMs: number;
  volume: number;
  error: string | null;
  trackId: number | null;
  /** 非空表示当前只是试听片段（秒） */
  trialSecs: number | null;
}

/** 与 Rust 侧 `LyricLineDto` 对齐。 */
export interface LyricLine {
  startMs: number;
  endMs: number;
  text: string;
  translation: string | null;
}

export interface LyricTrack {
  hasYrc: boolean;
  lines: LyricLine[];
}

/** 与 Rust 侧 `UserProfileDto` 对齐。 */
export interface UserProfile {
  userId: number;
  nickname: string;
  avatarUrl: string | null;
}

/** 与 Rust 侧 `PlaylistDto` 对齐。 */
export interface Playlist {
  id: number;
  name: string;
  coverUrl: string | null;
  trackCount: number;
  /** 5 = 官方「我喜欢的音乐」 */
  specialType?: number | null;
}

export type QrStatus = 'waiting' | 'scanned' | 'expired' | 'success';

/** 与 Rust 侧 `QueueDto` 对齐。 */
export interface QueueState {
  tracks: Track[];
  currentId: number | null;
  repeat: 'off' | 'all' | 'one';
  shuffle: boolean;
}

