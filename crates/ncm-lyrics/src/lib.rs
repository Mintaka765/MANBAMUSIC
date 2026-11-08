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

/// 把翻译轨合并进主轨（按行起点匹配，容忍 ±120ms 的抖动）。
pub fn merge_translation(track: &mut LyricTrack, translation: &LyricTrack) {
    for line in &mut track.lines {
        if let Some(found) = translation
            .lines
            .iter()
            .find(|other| other.start_ms.abs_diff(line.start_ms) <= 120)
        {
            if !found.text.trim().is_empty() {
                line.translation = Some(found.text.clone());
            }
        }
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
