//! 本地音乐：扫描用户选的文件夹，读标签生成曲目列表。
//!
//! 标签优先用 lofty；读不到就退回文件名，保证任何音频都能出现在列表里。

use std::path::{Path, PathBuf};

use lofty::file::{AudioFile, TaggedFileExt};
use lofty::tag::Accessor;

/// 支持的音频扩展名（小写）。
const AUDIO_EXTS: &[&str] = &[
    "mp3", "flac", "m4a", "aac", "wav", "ogg", "opus", "wma", "ape", "aiff", "aif",
];

/// 扫描结果：曲目 + 统计。
pub struct ScanOutcome {
    pub tracks: Vec<LocalTrack>,
}

pub struct LocalTrack {
    pub id: u64,
    pub path: String,
    pub name: String,
    pub artists: Vec<String>,
    pub album: String,
    pub duration_ms: u64,
}

/// FNV-1a：给路径一个稳定的 64 位 id（同一文件每次扫描都相同，队列持久化才能对上）。
fn hash_path(path: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in path.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    // 掩到 53 位：JS 的 number 超过 2^53 会丢精度，id 两边就对不上了
    hash & 0x001F_FFFF_FFFF_FFFF
}

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            let lower = ext.to_ascii_lowercase();
            AUDIO_EXTS.iter().any(|known| *known == lower)
        })
        .unwrap_or(false)
}

fn read_track(path: &Path) -> Option<LocalTrack> {
    let path_text = path.to_string_lossy().to_string();

    let fallback_name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("未知曲目")
        .to_string();

    match lofty::read_from_path(path) {
        Ok(tagged) => {
            let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
            let name = tag
                .and_then(|tag| tag.title())
                .map(|title| title.trim().to_string())
                .filter(|title| !title.is_empty())
                .unwrap_or(fallback_name);
            let artists: Vec<String> = tag
                .and_then(|tag| tag.artist())
                .map(|artist| {
                    artist
                        .split(['/', ';', '、'])
                        .map(|part| part.trim().to_string())
                        .filter(|part| !part.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            let album = tag
                .and_then(|tag| tag.album())
                .map(|album| album.trim().to_string())
                .unwrap_or_default();
            let duration_ms = tagged.properties().duration().as_millis() as u64;

            Some(LocalTrack {
                id: hash_path(&path_text),
                path: path_text,
                name,
                artists,
                album,
                duration_ms,
            })
        }
        Err(_) => {
            // 读不到标签也保留：至少能按文件名播放
            Some(LocalTrack {
                id: hash_path(&path_text),
                path: path_text,
                name: fallback_name,
                artists: Vec::new(),
                album: String::new(),
                duration_ms: 0,
            })
        }
    }
}

/// 递归扫描这些目录。
pub fn scan(folders: &[String]) -> ScanOutcome {
    let mut tracks = Vec::new();
    let mut skipped = 0usize;

    for folder in folders {
        let root = PathBuf::from(folder);
        if !root.is_dir() {
            continue;
        }
        for entry in walkdir::WalkDir::new(&root)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if !is_audio(path) {
                continue;
            }
            match read_track(path) {
                Some(track) => tracks.push(track),
                None => skipped += 1,
            }
        }
    }

    // 同名专辑里的曲目按文件名排，至少比随机顺序好读
    tracks.sort_by(|a, b| {
        a.album
            .cmp(&b.album)
            .then_with(|| a.path.cmp(&b.path))
    });

    if skipped > 0 {
        eprintln!("本地扫描：{skipped} 个文件读不出标签，已跳过");
    }
    ScanOutcome { tracks }
}
