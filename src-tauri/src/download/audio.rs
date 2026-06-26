// 音频提取
// 通过 yt-dlp + ffmpeg 实现，支持实时进度解析

use anyhow::{bail, Result};
use std::path::PathBuf;
use tokio::io::AsyncBufReadExt;
use tokio::process::Command;

use crate::utils;

/// 进度回调: (progress: 0.0~100.0, speed: String, eta: String)
pub type ProgressCallback = Box<dyn Fn(f64, String, String) + Send + 'static>;

/// 将 Cookie 字符串写入 Netscape 格式的 cookie 文件
fn write_cookie_file(cookie: &str) -> Result<PathBuf> {
    let cookie_path = std::env::temp_dir().join("bilibili-transcript-cookies.txt");

    let mut lines = vec!["# Netscape HTTP Cookie File".to_string()];

    for part in cookie.split(';') {
        let part = part.trim();
        if let Some((name, value)) = part.split_once('=') {
            let name = name.trim();
            let value = value.trim();
            lines.push(format!(".bilibili.com\tTRUE\t/\tFALSE\t0\t{}\t{}", name, value));
        }
    }

    std::fs::write(&cookie_path, lines.join("\n"))?;
    log::debug!("Cookie 文件已写入: {:?}", cookie_path);
    Ok(cookie_path)
}

/// 清理文件名中的格式 ID 后缀（如 .f30033.mp3 -> .mp3）
fn clean_format_suffix(path: &std::path::Path) -> std::path::PathBuf {
    let filename = match path.file_stem().and_then(|n| n.to_str()) {
        Some(f) => f,
        None => return path.to_path_buf(),
    };
    let ext = match path.extension().and_then(|e| e.to_str()) {
        Some(e) => e,
        None => return path.to_path_buf(),
    };

    let re = regex::Regex::new(r"\.f\d+$").unwrap();
    if re.is_match(filename) {
        let cleaned = re.replace(filename, "");
        if let Some(parent) = path.parent() {
            return parent.join(format!("{}.{}", cleaned, ext));
        }
    }

    path.to_path_buf()
}

/// 提取音频（下载 + 转 WAV，支持实时进度回调）
pub async fn extract_audio(
    url: &str,
    output_dir: &PathBuf,
    _cookie: &str,
    on_progress: Option<ProgressCallback>,
) -> Result<PathBuf> {
    log::info!("开始提取音频: {}", url);
    std::fs::create_dir_all(output_dir)?;

    // Step 1: 使用 yt-dlp 下载音频
    log::debug!("Step 1: yt-dlp 下载音频");
    let mp3_template = output_dir.join("%(title)s.%(ext)s");
    let mp3_str = mp3_template.to_string_lossy();

    let mut args = vec![
        "-x".to_string(),
        "--audio-format".to_string(),
        "mp3".to_string(),
        "-o".to_string(),
        mp3_str.to_string(),
        "--no-playlist".to_string(),
        "--newline".to_string(),  // 强制每行刷新
        "--restrict-filenames".to_string(),  // 将特殊字符替换为下划线，避免 [ ] 等字符导致文件操作失败
        "--user-agent".to_string(),
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36".to_string(),
        "--referer".to_string(),
        "https://www.bilibili.com".to_string(),
    ];

    // 优先使用浏览器 Cookie（更完整），避免 B站 412 反爬
    args.push("--cookies-from-browser".to_string());
    args.push("chrome".to_string());
    log::debug!("使用浏览器 Cookie: chrome");

    // 指定 ffmpeg 路径，解决 App bundle 启动时 PATH 不完整的问题
    match utils::resolve_ffmpeg_path() {
        Ok(ffmpeg_path) => {
            if let Some(parent) = ffmpeg_path.parent() {
                args.push("--ffmpeg-location".to_string());
                args.push(parent.to_string_lossy().to_string());
                log::debug!("ffmpeg 路径: {:?}", ffmpeg_path);
            }
        }
        Err(e) => {
            log::warn!("未找到 ffmpeg，yt-dlp 后处理可能失败: {}", e);
        }
    }

    args.push(url.to_string());

    log::debug!("执行 yt-dlp {:?}", args);

    let ytdlp_path = utils::resolve_ytdlp_path()?;
    log::debug!("yt-dlp 路径: {:?}", ytdlp_path);
    let mut child = Command::new(&ytdlp_path)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    // 异步读取 stderr，避免 pipe 缓冲区满导致死锁
    let stderr_pipe = child.stderr.take().expect("stderr should be piped");
    let stderr_handle = tokio::spawn(async move {
        let reader = tokio::io::BufReader::new(stderr_pipe);
        let mut lines = reader.lines();
        let mut stderr_lines = Vec::new();
        while let Ok(Some(line)) = lines.next_line().await {
            stderr_lines.push(line);
        }
        stderr_lines
    });

    // 异步读取 stdout，解析进度
    let stdout_pipe = child.stdout.take().expect("stdout should be piped");
    let reader = tokio::io::BufReader::new(stdout_pipe);
    let mut lines = reader.lines();

    let mut last_progress = 0.0f64;
    let mut stdout_lines: Vec<String> = Vec::new();
    let mut final_path: Option<PathBuf> = None;

    while let Ok(Some(line)) = lines.next_line().await {
        // 解析 yt-dlp 进度（输出到 stdout）
        if line.contains("[download]") && line.contains('%') {
            if let Some(pct_str) = line.split_whitespace().find(|s| s.ends_with('%')) {
                let pct_str = pct_str.trim_end_matches('%');
                if let Ok(pct) = pct_str.parse::<f64>() {
                    let speed = line.split("at ").nth(1)
                        .and_then(|s| s.split_whitespace().next())
                        .unwrap_or("")
                        .to_string();
                    let eta = line.split("ETA ").nth(1)
                        .unwrap_or("")
                        .trim()
                        .to_string();

                    if pct > last_progress {
                        last_progress = pct;
                        if let Some(ref cb) = on_progress {
                            cb(pct, speed, eta);
                        }
                    }
                }
            }
        }

        // 匹配文件路径（多种格式）
        let trimmed = line.trim();
        if trimmed.starts_with('/') && !trimmed.contains('[') && !trimmed.contains('%') {
            // --print after_move:filepath 输出的纯路径行
            final_path = Some(PathBuf::from(trimmed));
        } else if trimmed.contains("[download]") && trimmed.contains("has already been downloaded") {
            // [download] /path/to/file.mp3 has already been downloaded
            if let Some(path_str) = trimmed.split("[download]").nth(1) {
                let path_str = path_str.split("has already been downloaded").next().unwrap_or("").trim();
                final_path = Some(PathBuf::from(path_str));
            }
        } else if trimmed.contains("[ExtractAudio]") && trimmed.contains("Not converting audio") {
            // [ExtractAudio] Not converting audio /path/to/file.mp3; file is already in target format mp3
            if let Some(path_str) = trimmed.split("Not converting audio").nth(1) {
                let path_str = path_str.split(';').next().unwrap_or("").trim();
                final_path = Some(PathBuf::from(path_str));
            }
        } else if trimmed.contains("[ExtractAudio]") && trimmed.contains("Destination:") {
            // [ExtractAudio] Destination: /path/to/file.mp3
            if let Some(path_str) = trimmed.split("Destination:").nth(1) {
                final_path = Some(PathBuf::from(path_str.trim()));
            }
        }

        stdout_lines.push(line);
    }

    let status = child.wait().await?;
    let stderr_lines = stderr_handle.await.unwrap_or_default();

    if !status.success() {
        let err_msg = stdout_lines.join("\n");
        let stderr_msg = stderr_lines.join("\n");
        log::error!("yt-dlp 音频下载失败: stdout={}, stderr={}", err_msg, stderr_msg);
        bail!("yt-dlp 音频下载失败: {}", if err_msg.is_empty() { stderr_msg } else { err_msg });
    }

    // 记录 stderr 中的警告信息
    for line in &stderr_lines {
        if !line.trim().is_empty() {
            log::warn!("yt-dlp stderr: {}", line);
        }
    }

    let stdout = stdout_lines.join("\n");
    log::debug!("yt-dlp 输出: {}", stdout);

    // 等待文件写入完成
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let mp3_path = final_path
        .or_else(|| find_downloaded_file_from_lines(&stdout_lines, output_dir, "mp3"))
        .ok_or_else(|| anyhow::anyhow!("无法找到下载的音频文件"))?;

    let mp3_path = clean_format_suffix(&mp3_path);
    log::debug!("音频文件: {:?}", mp3_path);

    // 如果文件已经是 WAV 格式，跳过转换
    let wav_path = mp3_path.with_extension("wav");
    if mp3_path.extension().and_then(|e| e.to_str()) == Some("wav") {
        log::debug!("音频已是 WAV 格式，跳过 ffmpeg 转换");
        if mp3_path != wav_path {
            std::fs::copy(&mp3_path, &wav_path)?;
        }
    } else {
        // Step 2: 使用 ffmpeg 转为 16kHz 单声道 WAV
        log::debug!("Step 2: ffmpeg 转换为 16kHz WAV");
        let ffmpeg_path = utils::resolve_ffmpeg_path()?;
        log::debug!("ffmpeg 路径: {:?}", ffmpeg_path);
        let ffmpeg_output = Command::new(&ffmpeg_path)
            .args([
                "-y",
                "-i",
                &mp3_path.to_string_lossy(),
                "-ar",
                "16000",
                "-ac",
                "1",
                "-c:a",
                "pcm_s16le",
                &wav_path.to_string_lossy(),
            ])
            .output()
            .await?;

        if !ffmpeg_output.status.success() {
            let stderr = String::from_utf8_lossy(&ffmpeg_output.stderr);
            log::error!("ffmpeg 音频转换失败: {}", stderr);
            bail!("ffmpeg 音频转换失败: {}", stderr);
        }

        let _ = std::fs::remove_file(&mp3_path);
    }

    log::info!("音频提取完成: {:?}", wav_path);
    Ok(wav_path)
}

/// 仅下载音频（不转换格式，支持实时进度回调）
pub async fn download_audio(
    url: &str,
    output_dir: &PathBuf,
    _cookie: &str,
    on_progress: Option<ProgressCallback>,
) -> Result<PathBuf> {
    log::info!("下载音频: {}", url);
    std::fs::create_dir_all(output_dir)?;

    let output_template = output_dir.join("%(title)s.%(ext)s");
    let output_str = output_template.to_string_lossy();

    let mut args = vec![
        "-x".to_string(),
        "--audio-format".to_string(),
        "mp3".to_string(),
        "-o".to_string(),
        output_str.to_string(),
        "--no-playlist".to_string(),
        "--newline".to_string(),
        "--user-agent".to_string(),
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36".to_string(),
        "--referer".to_string(),
        "https://www.bilibili.com".to_string(),
    ];

    // 优先使用浏览器 Cookie（更完整），避免 B站 412 反爬
    args.push("--cookies-from-browser".to_string());
    args.push("chrome".to_string());

    // 指定 ffmpeg 路径，解决 App bundle 启动时 PATH 不完整的问题
    match utils::resolve_ffmpeg_path() {
        Ok(ffmpeg_path) => {
            if let Some(parent) = ffmpeg_path.parent() {
                args.push("--ffmpeg-location".to_string());
                args.push(parent.to_string_lossy().to_string());
                log::debug!("ffmpeg 路径: {:?}", ffmpeg_path);
            }
        }
        Err(e) => {
            log::warn!("未找到 ffmpeg，yt-dlp 后处理可能失败: {}", e);
        }
    }

    args.push(url.to_string());

    log::debug!("执行 yt-dlp {:?}", args);

    let ytdlp_path = utils::resolve_ytdlp_path()?;
    log::debug!("yt-dlp 路径: {:?}", ytdlp_path);
    let mut child = Command::new(&ytdlp_path)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    // 异步读取 stderr，避免 pipe 缓冲区满导致死锁
    let stderr_pipe = child.stderr.take().expect("stderr should be piped");
    let stderr_handle = tokio::spawn(async move {
        let reader = tokio::io::BufReader::new(stderr_pipe);
        let mut lines = reader.lines();
        let mut stderr_lines = Vec::new();
        while let Ok(Some(line)) = lines.next_line().await {
            stderr_lines.push(line);
        }
        stderr_lines
    });

    let stdout_pipe = child.stdout.take().expect("stdout should be piped");
    let reader = tokio::io::BufReader::new(stdout_pipe);
    let mut lines = reader.lines();

    let mut last_progress = 0.0f64;
    let mut stdout_lines: Vec<String> = Vec::new();
    let mut final_path: Option<PathBuf> = None;

    while let Ok(Some(line)) = lines.next_line().await {
        if line.contains("[download]") && line.contains('%') {
            if let Some(pct_str) = line.split_whitespace().find(|s| s.ends_with('%')) {
                let pct_str = pct_str.trim_end_matches('%');
                if let Ok(pct) = pct_str.parse::<f64>() {
                    let speed = line.split("at ").nth(1)
                        .and_then(|s| s.split_whitespace().next())
                        .unwrap_or("")
                        .to_string();
                    let eta = line.split("ETA ").nth(1)
                        .unwrap_or("")
                        .trim()
                        .to_string();

                    if pct > last_progress {
                        last_progress = pct;
                        if let Some(ref cb) = on_progress {
                            cb(pct, speed, eta);
                        }
                    }
                }
            }
        }

        let trimmed = line.trim();
        // --print after_move:filepath 输出的纯路径行（非 [ 开头，非空，不包含特殊标记）
        if !trimmed.is_empty() && !trimmed.starts_with('[') && !trimmed.contains("Destination:") && !trimmed.contains('%') && trimmed.starts_with('/') {
            final_path = Some(PathBuf::from(trimmed));
        }

        stdout_lines.push(line);
    }

    let status = child.wait().await?;
    let stderr_lines = stderr_handle.await.unwrap_or_default();

    log::debug!("yt-dlp exit status: {}", status);
    log::debug!("yt-dlp stdout lines count: {}", stdout_lines.len());
    for (i, line) in stdout_lines.iter().enumerate() {
        log::debug!("yt-dlp stdout[{}]: {}", i, line);
    }
    log::debug!("yt-dlp stderr lines count: {}", stderr_lines.len());
    for (i, line) in stderr_lines.iter().enumerate() {
        log::debug!("yt-dlp stderr[{}]: {}", i, line);
    }
    log::debug!("yt-dlp final_path: {:?}", final_path);

    if !status.success() {
        let err_msg = stdout_lines.join("\n");
        let stderr_msg = stderr_lines.join("\n");
        log::error!("yt-dlp 音频下载失败: stdout={}, stderr={}", err_msg, stderr_msg);
        bail!("yt-dlp 音频下载失败: {}", if err_msg.is_empty() { stderr_msg } else { err_msg });
    }

    // 记录 stderr 中的警告信息
    for line in &stderr_lines {
        if !line.trim().is_empty() {
            log::warn!("yt-dlp stderr: {}", line);
        }
    }

    let filepath = final_path
        .or_else(|| find_downloaded_file_from_lines(&stdout_lines, output_dir, "mp3"))
        .ok_or_else(|| anyhow::anyhow!("无法找到下载的音频文件"))?;

    log::debug!("解析到文件路径: {:?}", filepath);

    // 等待文件写入完成（yt-dlp 可能需要一点时间完成文件移动）
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let clean_path = clean_format_suffix(&filepath);
    if clean_path != filepath {
        if let Err(e) = std::fs::rename(&filepath, &clean_path) {
            log::warn!("重命名文件失败（保留原文件名）: {}", e);
        } else {
            log::debug!("文件名已清理: {:?} -> {:?}", filepath, clean_path);
        }
    }

    let result = if clean_path.exists() { clean_path } else { filepath };
    log::info!("音频下载完成: {:?}", result);
    Ok(result)
}

/// 从输出行中查找下载的文件
fn find_downloaded_file_from_lines(lines: &[String], _output_dir: &PathBuf, _ext: &str) -> Option<PathBuf> {
    // 优先匹配: --print after_move:filepath 输出的纯路径行
    for line in lines.iter().rev() {
        let trimmed = line.trim();
        if !trimmed.is_empty() && trimmed.starts_with('/') && !trimmed.contains('[') && !trimmed.contains('%') {
            let p = PathBuf::from(trimmed);
            if p.exists() {
                log::debug!("从 --print 输出获取文件路径: {:?}", p);
                return Some(p);
            }
        }
    }

    // 次匹配: [download] /path/to/file.mp3 has already been downloaded
    for line in lines.iter().rev() {
        if line.contains("[download]") && line.contains("has already been downloaded") {
            if let Some(path_str) = line.split("[download]").nth(1) {
                let path_str = path_str.split("has already been downloaded").next().unwrap_or("").trim();
                let p = PathBuf::from(path_str);
                if p.exists() {
                    log::debug!("从 'already downloaded' 获取文件路径: {:?}", p);
                    return Some(p);
                }
            }
        }
    }

    // 次匹配: [ExtractAudio] Not converting audio /path/to/file.mp3
    for line in lines.iter().rev() {
        if line.contains("[ExtractAudio]") && line.contains("Not converting audio") {
            if let Some(path_str) = line.split("Not converting audio").nth(1) {
                let path_str = path_str.split(';').next().unwrap_or("").trim();
                let p = PathBuf::from(path_str);
                if p.exists() {
                    log::debug!("从 ExtractAudio 获取文件路径: {:?}", p);
                    return Some(p);
                }
            }
        }
    }

    // 次匹配: [ExtractAudio] Destination: /path/to/file.mp3
    for line in lines.iter().rev() {
        if line.contains("[ExtractAudio]") && line.contains("Destination:") {
            if let Some(path_str) = line.split("Destination:").nth(1) {
                let p = PathBuf::from(path_str.trim());
                if p.exists() {
                    log::debug!("从 ExtractAudio Destination 获取文件路径: {:?}", p);
                    return Some(p);
                }
            }
        }
    }

    // 次匹配: [download] Destination: /path/to/file
    for line in lines.iter().rev() {
        if line.contains("[download]") && line.contains("Destination:") {
            if let Some(path_str) = line.split("Destination:").nth(1) {
                let p = PathBuf::from(path_str.trim());
                if p.exists() {
                    log::debug!("从 download Destination 获取文件路径: {:?}", p);
                    return Some(p);
                }
            }
        }
    }

    None
}
