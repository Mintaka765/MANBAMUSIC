//! 歌词解析。
//!
//! 两种格式：
//! - **LRC**：`[mm:ss.xxx]文本`，一行可带多个时间戳；
//! - **YRC**（逐字）：`[行起点,行时长](词起点,词时长,flag)字…`。
//!
//! 解析结果是统一的时间轴，前端拿当前播放位置二分即可，不需要每帧问后端。

/// 一个字/词的片段，用于逐字高亮。
#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LyricLine {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub words: Vec<Word>,
    pub translation: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LyricTrack {
    pub lines: Vec<LyricLine>,
    /// 是否来自逐字歌词（否则是整行推进）。
    pub has_yrc: bool,
}

impl LyricTrack {
    /// 当前该高亮第几行；`None` 表示还没到第一行。
    pub fn line_at(&self, position_ms: u64) -> Option<usize> {
        if self.lines.is_empty() {
            return None;
        }
        // partition_point：第一个 start_ms > position 的下标，减一即当前行
        let idx = self
            .lines
            .partition_point(|line| line.start_ms <= position_ms);
        if idx == 0 {
            None
        } else {
            Some(idx - 1)
        }
    }
}

/// 解析 LRC。返回按时间排序的歌词行（不含翻译）。
pub fn parse_lrc(raw: &str) -> LyricTrack {
    let mut lines = Vec::new();
    let mut offset_ms: i64 = 0;

    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // 元信息：[offset:+500] 之类
        if let Some(rest) = line.strip_prefix("[offset:") {
            if let Ok(value) = rest.trim_end_matches(']').trim().parse::<i64>() {
                offset_ms = value;
            }
            continue;
        }

        let (timestamps, text) = split_timestamps(line);
        if timestamps.is_empty() {
            continue;
        }
        let text = text.trim().to_string();
        if text.is_empty() {
            continue;
        }

        for start_ms in timestamps {
            let adjusted = (start_ms as i64 + offset_ms).max(0) as u64;
            lines.push(LyricLine {
                start_ms: adjusted,
                end_ms: adjusted,
                text: text.clone(),
                words: Vec::new(),
                translation: None,
            });
        }
    }

    lines.sort_by_key(|line| line.start_ms);
    fill_ends(&mut lines, 5_000);
    LyricTrack {
        lines,
        has_yrc: false,
    }
}

/// 解析 YRC（逐字）。格式：`[start,dur](wstart,wdur,flag)字…`
pub fn parse_yrc(raw: &str) -> LyricTrack {
    let mut lines = Vec::new();

    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || !line.starts_with('[') {
            continue;
        }

        let Some(close) = line.find(']') else {
            continue;
        };
        let header = &line[1..close];
        let Some((start, duration)) = parse_pair(header) else {
            continue;
        };
        let start_ms = start;
        let end_ms = start + duration;

        let mut words = Vec::new();
        let mut text = String::new();
        let mut rest = &line[close + 1..];

        while let Some(open) = rest.find('(') {
            text.push_str(&rest[..open]);
            let Some(close_paren) = rest[open..].find(')') else {
                break;
            };
            let meta = &rest[open + 1..open + close_paren];
            let after = open + close_paren + 1;

            let next_open = rest[after..]
                .find('(')
                .map(|i| after + i)
                .unwrap_or(rest.len());
            let segment = &rest[after..next_open];

            if let Some((w_start, w_duration)) = parse_pair(meta) {
                if !segment.is_empty() {
                    words.push(Word {
                        start_ms: w_start,
                        end_ms: w_start + w_duration,
                        text: segment.to_string(),
                    });
                    text.push_str(segment);
                }
            }
            rest = &rest[next_open..];
        }

        if text.trim().is_empty() {
            continue;
        }

        lines.push(LyricLine {
            start_ms,
            end_ms,
            text,
            words,
            translation: None,
        });
    }

    lines.sort_by_key(|line| line.start_ms);
    LyricTrack {
        lines,
        has_yrc: true,
    }
}

/// 主轨与翻译轨时间戳允许的偏差。
///
/// 实测翻译轨和主歌词经常不是同一份时间轴：同一句会差几百毫秒甚至 1.4 秒
/// （例：Lemon 最大 851ms、夜に駆ける 1430ms）。窗口太小会让这些句子显示成
/// 「没有翻译」。
const TRANSLATION_TOLERANCE_MS: u64 = 1_500;

/// 把翻译轨合并进主轨：按时间取最近的一句，且一句翻译只匹配一次。
pub fn merge_translation(track: &mut LyricTrack, translation: &LyricTrack) {
    if translation.lines.is_empty() {
        return;
    }

    // 行数相同说明两边是同一份歌词的两种时间轴（逐字轨和翻译轨经常差几百毫秒），
    // 直接按顺序对齐最稳，不依赖时间戳。
    if translation.lines.len() == track.lines.len() {
        for (line, other) in track.lines.iter_mut().zip(translation.lines.iter()) {
            let text = other.text.trim();
            if !text.is_empty() && text != line.text.trim() {
                line.translation = Some(text.to_string());
            }
        }
        return;
    }

    let mut used = vec![false; translation.lines.len()];

    for line in &mut track.lines {
        // 候选只可能是插入点两侧的两句（翻译轨按时间有序）
        let at = translation
            .lines
            .partition_point(|other| other.start_ms < line.start_ms);

        let mut best: Option<(usize, u64)> = None;
        for index in [at.saturating_sub(1), at] {
            let Some(other) = translation.lines.get(index) else {
                continue;
            };
            if used[index] {
                continue;
            }
            let delta = other.start_ms.abs_diff(line.start_ms);
            if delta > TRANSLATION_TOLERANCE_MS {
                continue;
            }
            if best.is_none_or(|(_, previous)| delta < previous) {
                best = Some((index, delta));
            }
        }

        let Some((index, _)) = best else {
            continue;
        };
        let text = translation.lines[index].text.trim();
        if text.is_empty() || text == line.text.trim() {
            continue;
        }
        used[index] = true;
        line.translation = Some(text.to_string());
    }
}

/// 兜底：LRC 没有行结束时间，用下一行起点补，最后一行给 5 秒。
fn fill_ends(lines: &mut [LyricLine], tail_ms: u64) {
    let len = lines.len();
    for index in 0..len {
        if lines[index].end_ms <= lines[index].start_ms {
            let end = if index + 1 < len {
                lines[index + 1].start_ms
            } else {
                lines[index].start_ms + tail_ms
            };
            lines[index].end_ms = end;
            let start = lines[index].start_ms;
            lines[index].words = vec![Word {
                start_ms: start,
                end_ms: end,
                text: lines[index].text.clone(),
            }];
        }
    }
}

fn parse_pair(input: &str) -> Option<(u64, u64)> {
    let mut parts = input.split(',');
    let first = parts.next()?.trim().parse().ok()?;
    let second = parts.next()?.trim().parse().ok()?;
    Some((first, second))
}

/// 拆出 `[mm:ss.xxx][mm:ss.xxx]文本` 里的时间戳与正文。
fn split_timestamps(line: &str) -> (Vec<u64>, &str) {
    let mut timestamps = Vec::new();
    let mut rest = line;

    while let Some(stripped) = rest.strip_prefix('[') {
        let Some(close) = stripped.find(']') else {
            break;
        };
        let inner = &stripped[..close];
        match parse_timestamp(inner) {
            Some(ms) => {
                timestamps.push(ms);
                rest = &stripped[close + 1..];
            }
            None => break,
        }
    }

    (timestamps, rest)
}

/// `mm:ss.xxx` / `mm:ss:xxx` / `mm:ss` → 毫秒
fn parse_timestamp(input: &str) -> Option<u64> {
    let (minutes, rest) = input.split_once(':')?;
    let minutes: u64 = minutes.trim().parse().ok()?;

    let (seconds, fraction) = if let Some((s, f)) = rest.split_once('.') {
        (s, f)
    } else if let Some((s, f)) = rest.split_once(':') {
        (s, f)
    } else {
        (rest, "")
    };

    let seconds: u64 = seconds.trim().parse().ok()?;
    let millis = if fraction.is_empty() {
        0
    } else {
        let mut digits = fraction.trim().to_string();
        digits.truncate(3);
        while digits.len() < 3 {
            digits.push('0');
        }
        digits.parse().ok()?
    };

    Some(minutes * 60_000 + seconds * 1_000 + millis)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LRC: &str =
        "[00:00.000] 作词 : 周杰伦\n[00:12.500]故事的小黄花\n[01:05.20]从出生那年就飘着\n";

    const YRC: &str = "[28480,11820](28480,160,0)我(28640,420,0)带(29060,230,0)着";

    #[test]
    fn lrc_parses_timestamps_and_fills_ends() {
        let track = parse_lrc(LRC);
        assert_eq!(track.lines.len(), 3);
        assert_eq!(track.lines[1].start_ms, 12_500);
        assert_eq!(track.lines[1].end_ms, 65_200, "用下一行起点补结束时间");
        assert!(!track.has_yrc);
    }

    #[test]
    fn lrc_supports_multiple_timestamps_per_line() {
        let track = parse_lrc("[00:01.000][00:02.000]重复");
        assert_eq!(track.lines.len(), 2);
        assert_eq!(track.lines[0].start_ms, 1_000);
        assert_eq!(track.lines[1].start_ms, 2_000);
    }

    #[test]
    fn lrc_honours_offset_tag() {
        let track = parse_lrc("[offset:500]\n[00:01.000]偏移");
        assert_eq!(track.lines[0].start_ms, 1_500);
    }

    #[test]
    fn yrc_parses_words() {
        let track = parse_yrc(YRC);
        assert_eq!(track.lines.len(), 1);
        let line = &track.lines[0];
        assert_eq!(line.start_ms, 28_480);
        assert_eq!(line.end_ms, 28_480 + 11_820);
        assert_eq!(line.text, "我带着");
        assert_eq!(line.words.len(), 3);
        assert_eq!(line.words[0].text, "我");
        assert_eq!(line.words[2].start_ms, 29_060);
        assert!(track.has_yrc);
    }

    #[test]
    fn line_at_uses_binary_search() {
        let track = parse_lrc("[00:01.000]a\n[00:10.000]b\n[00:20.000]c");
        assert_eq!(track.line_at(500), None);
        assert_eq!(track.line_at(1_000), Some(0));
        assert_eq!(track.line_at(15_000), Some(1));
        assert_eq!(track.line_at(99_999), Some(2));
    }

    #[test]
    fn translation_matches_within_tolerance() {
        let mut main = parse_lrc("[00:10.000]你好");
        let trans = parse_lrc("[00:10.080]Hello");
        merge_translation(&mut main, &trans);
        assert_eq!(main.lines[0].translation.as_deref(), Some("Hello"));
    }
}

#[cfg(test)]
mod translation_tests {
    use super::*;

    #[test]
    fn matches_large_timestamp_drift() {
        let mut main = parse_lrc("[00:10.000]你好");
        let trans = parse_lrc("[00:10.851]Hello");
        merge_translation(&mut main, &trans);
        assert_eq!(main.lines[0].translation.as_deref(), Some("Hello"));
    }

    #[test]
    fn does_not_match_beyond_tolerance() {
        // 行数不同才走时间匹配：这句翻译离得太远，不该贴上去
        let mut main = parse_lrc("[00:10.000]你好\n[00:20.000]第二句");
        let trans = parse_lrc("[00:30.000]Far away");
        merge_translation(&mut main, &trans);
        assert!(main.lines.iter().all(|line| line.translation.is_none()));
    }

    #[test]
    fn equal_counts_align_by_index() {
        // 逐字轨与翻译轨行数相同但时间戳差很多，应按顺序对齐
        let mut main = parse_lrc("[00:10.000]第一句\n[00:20.000]第二句");
        let trans = parse_lrc("[00:10.500]First\n[00:23.400]Second");
        merge_translation(&mut main, &trans);
        assert_eq!(main.lines[0].translation.as_deref(), Some("First"));
        assert_eq!(main.lines[1].translation.as_deref(), Some("Second"));
    }

    #[test]
    fn translation_is_not_reused() {
        let mut main = parse_lrc("[00:10.000]a\n[00:10.900]b");
        let trans = parse_lrc("[00:10.400]only one");
        merge_translation(&mut main, &trans);
        let used = main
            .lines
            .iter()
            .filter(|line| line.translation.is_some())
            .count();
        assert_eq!(used, 1, "一句翻译只能贴到一行");
    }

    #[test]
    fn identical_text_is_skipped() {
        let mut main = parse_lrc("[00:10.000]same");
        let trans = parse_lrc("[00:10.000]same");
        merge_translation(&mut main, &trans);
        assert!(main.lines[0].translation.is_none());
    }
}