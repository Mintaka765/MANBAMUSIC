//! M0 验收工具：协议层不依赖 UI 与音频也能跑通。
//!
//! ```text
//! ncm-cli search 晴天 --limit 5
//! ncm-cli url 186016 --level exhigh
//! ncm-cli lyric 186016
//! ```

use std::error::Error;

use ncm_api::{endpoints, Client};

const USAGE: &str = "\
用法:
  ncm-cli search <关键词> [--limit N]     搜索单曲
  ncm-cli url <歌曲ID> [--level L]        解析播放直链（默认 exhigh）
  ncm-cli play <歌曲ID> [--level L] [--seconds N] [--probe]
                                          播放；--probe 只解码不发声
  ncm-cli lyric <歌曲ID>                  检查歌词：lrc / yrc / tlyric 是否可用

level 取值: standard | higher | exhigh | lossless（会自动降级）";

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(err) = run(&args).await {
        eprintln!("错误: {err}");
        std::process::exit(1);
    }
}

async fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    let command = args.first().map(String::as_str).unwrap_or("help");
    if matches!(command, "help" | "-h" | "--help") {
        println!("{USAGE}");
        return Ok(());
    }

    let client = Client::new()?;
    println!("[deviceId={}]", client.device_id());

    match command {
        "search" => {
            let keyword = args
                .get(1)
                .ok_or("用法: ncm-cli search <关键词> [--limit N]")?;
            let limit = flag_value(args, "--limit")
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(10)
                .clamp(1, 50);

            let songs = endpoints::search(&client, keyword, limit).await?;
            println!("搜索「{keyword}」：{} 条", songs.len());
            for (idx, song) in songs.iter().enumerate() {
                println!(
                    "{:>3}. {:>12}  {:<32} {:<20} {:>6}  fee={}",
                    idx + 1,
                    song.id,
                    truncate(&song.name, 32),
                    truncate(&song.artists.join("/"), 20),
                    format_duration(song.duration_ms),
                    song.fee
                );
            }
        }

        "url" => {
            let id: u64 = args
                .get(1)
                .ok_or("用法: ncm-cli url <歌曲ID>")?
                .parse()
                .map_err(|_| "歌曲 ID 必须是数字")?;
            let level = flag_value(args, "--level").unwrap_or("exhigh");
            let ladder: Vec<&str> = match level {
                "lossless" => vec!["lossless", "exhigh", "standard"],
                "exhigh" | "higher" => vec!["exhigh", "standard"],
                _ => vec!["standard"],
            };

            let resolved = endpoints::play_url_with_fallback(&client, id, &ladder).await?;
            println!("level     : {}", resolved.level);
            println!("br        : {} bps", resolved.br);
            println!("size      : {} bytes", resolved.size);
            println!(
                "expi      : {} 秒（{} 分钟后失效）",
                resolved.expi_secs,
                resolved.expi_secs / 60
            );
            println!("gain/peak : {:?} / {:?}", resolved.gain, resolved.peak);
            println!("url       : {}", resolved.url);
        }

        "lyric" => {
            let id: u64 = args
                .get(1)
                .ok_or("用法: ncm-cli lyric <歌曲ID>")?
                .parse()
                .map_err(|_| "歌曲 ID 必须是数字")?;

            let lyric = endpoints::lyric(&client, id).await?;
            let lines = |text: &Option<String>| {
                text.as_ref()
                    .map(|t| t.lines().filter(|l| !l.trim().is_empty()).count())
                    .unwrap_or(0)
            };
            println!("lrc     : {} 行", lines(&lyric.lrc));
            println!(
                "yrc     : {} 行（0 表示该曲没有逐字歌词，需回落 lrc）",
                lines(&lyric.yrc)
            );
            println!("tlyric  : {} 行", lines(&lyric.tlyric));
            println!("romalrc : {} 行", lines(&lyric.romalrc));

            let sample = lyric
                .yrc
                .as_ref()
                .or(lyric.lrc.as_ref())
                .map(|t| t.lines().take(3).collect::<Vec<_>>().join("\n"))
                .unwrap_or_default();
            if !sample.is_empty() {
                println!("--- 前几行 ---\n{sample}");
            }
        }

        "play" => {
            let id: u64 = args
                .get(1)
                .ok_or("用法: ncm-cli play <歌曲ID> [--level L] [--seconds N] [--probe]")?
                .parse()
                .map_err(|_| "歌曲 ID 必须是数字")?;
            let level = flag_value(args, "--level").unwrap_or("exhigh");
            let ladder: Vec<&str> = match level {
                "lossless" => vec!["lossless", "exhigh", "standard"],
                "exhigh" | "higher" => vec!["exhigh", "standard"],
                _ => vec!["standard"],
            };

            let resolved = endpoints::play_url_with_fallback(&client, id, &ladder).await?;
            println!(
                "直链 {} kbps，{} 秒后过期（{}）",
                resolved.br / 1000,
                resolved.expi_secs,
                resolved.level
            );

            if args.iter().any(|arg| arg == "--probe") {
                let cache_dir = std::env::temp_dir().join("manbamusic-cache");
                let info = ncm_audio::probe(&resolved.url, &cache_dir).await?;
                println!(
                    "解码成功: 时长 {:.1}s, {} Hz, {} 声道",
                    info.duration.map(|d| d.as_secs_f32()).unwrap_or(-1.0),
                    info.sample_rate,
                    info.channels
                );
                return Ok(());
            }

            let limit = flag_value(args, "--seconds")
                .and_then(|value| value.parse::<u64>().ok())
                .map(std::time::Duration::from_secs);

            println!("开始播放（Ctrl+C 结束）...");
            let cache_dir = std::env::temp_dir().join("manbamusic-cache");
            match ncm_audio::play_url(&resolved.url, &cache_dir, limit).await {
                Ok(()) => println!("播放结束"),
                Err(ncm_audio::AudioError::Device(message)) => {
                    println!("没有可用的音频输出设备: {message}");
                    println!("可以加 --probe 只验证解码链路");
                }
                Err(err) => return Err(err.into()),
            }
        }

        other => return Err(format!("未知命令: {other}\n\n{USAGE}").into()),
    }

    Ok(())
}

fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let pos = args.iter().position(|a| a == flag)?;
    args.get(pos + 1).map(String::as_str)
}

fn format_duration(ms: u64) -> String {
    let total_secs = ms / 1000;
    format!("{}:{:02}", total_secs / 60, total_secs % 60)
}

/// 按字符（不是字节）截断，避免切碎中文。
fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}
