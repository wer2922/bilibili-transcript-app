// 视频下载
// 通过 yt-dlp 调用实现，支持实时进度解析

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

/// 下载视频（支持实时进度回调）
pub async fn download_video(
    url: &str,
    format_id: &str,
    output_dir: &PathBuf,
    _cookie: &str,
    on_progress: Option<ProgressCallback>,
) -> Result<PathBuf> {
    log::info!("开始下载视频: url={}, format={}", url, format_id);
    log::debug!("输出目录: {:?}", output_dir);

    std::fs::create_dir_all(output_dir)?;

    let output_template = output_dir.join("%(title)s.%(ext)s");
    let output_str = output_template.to_string_lossy();

    let mut args = vec![
        "-o".to_string(),
        output_str.to_string(),
        "--no-playlist".to_string(),
        "--newline".to_string(),  // 强制每行刷新，确保进度输出到 stderr
        "--restrict-filenames".to_string(),  // 将特殊字符替换为下划线，避免 [ ] 等字符导致文件操作失败
        "--user-agent".to_string(),
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36".to_string(),
        "--referer".to_string(),
        "https://www.bilibili.com".to_string(),
    ];

    // 格式选择
    if !format_id.is_empty() && format_id != "best" {
        args.push("-f".to_string());
        args.push(format_id.to_string());
    }

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

    // 异步读取 stdout，解析进度（yt-dlp --newline 把进度输出到 stdout）
    let stdout = child.stdout.take().expect("stdout should be piped");
    let reader = tokio::io::BufReader::new(stdout);
    let mut lines = reader.lines();

    let mut last_progress = 0.0f64;
    let mut stdout_lines: Vec<String> = Vec::new();

    while let Ok(Some(line)) = lines.next_line().await {
        // 解析 yt-dlp 进度: [download] 45.2% of 50.0MiB at 2.5MiB/s ETA 00:15
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
        stdout_lines.push(line);
    }

    let status = child.wait().await?;
    let stderr_lines = stderr_handle.await.unwrap_or_default();

    if !status.success() {
        let err_msg = stdout_lines.join("\n");
        let stderr_msg = stderr_lines.join("\n");
        log::error!("yt-dlp 下载失败: stdout={}, stderr={}", err_msg, stderr_msg);
        bail!("yt-dlp 下载失败: {}", if err_msg.is_empty() { stderr_msg } else { err_msg });
    }

    // 记录 stderr 中的警告信息
    for line in &stderr_lines {
        if !line.trim().is_empty() {
            log::warn!("yt-dlp stderr: {}", line);
        }
    }

    let stdout = stdout_lines.join("\n");
    log::debug!("yt-dlp 输出: {}", stdout);
    let filepath = parse_download_path(&stdout, output_dir)
        .unwrap_or_else(|| output_dir.join("download.mp4"));

    // 清理文件名中的格式 ID 后缀（如 .f30033）
    let clean_path = clean_format_suffix(&filepath);
    if clean_path != filepath {
        if let Err(e) = std::fs::rename(&filepath, &clean_path) {
            log::warn!("重命名文件失败（保留原文件名）: {}", e);
        } else {
            log::debug!("文件名已清理: {:?} -> {:?}", filepath, clean_path);
        }
    }

    let result = if clean_path.exists() { clean_path } else { filepath };
    log::info!("视频下载完成: {:?}", result);
    Ok(result)
}

/// 获取视频可用格式列表
pub async fn list_formats(url: &str, _cookie: &str) -> Result<Vec<serde_json::Value>> {
    log::info!("获取视频格式列表: {}", url);

    let mut args = vec![
        "-F".to_string(),
        "--dump-json".to_string(),
        "--user-agent".to_string(),
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36".to_string(),
        "--referer".to_string(),
        "https://www.bilibili.com".to_string(),
    ];

    // 优先使用浏览器 Cookie
    args.push("--cookies-from-browser".to_string());
    args.push("chrome".to_string());

    args.push(url.to_string());

    let ytdlp_path = utils::resolve_ytdlp_path()?;
    log::debug!("yt-dlp 路径: {:?}", ytdlp_path);
    let output = tokio::process::Command::new(&ytdlp_path)
        .args(&args)
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        log::error!("yt-dlp 获取格式失败: {}", stderr);
        bail!("yt-dlp 获取格式失败: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut formats = Vec::new();

    for line in stdout.lines() {
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(line) {
            formats.push(data);
        }
    }

    log::info!("格式列表获取成功: {} 个格式", formats.len());
    Ok(formats)
}

/// 清理文件名中的格式 ID 后缀（如 .f30033.mp4 -> .mp4）
fn clean_format_suffix(path: &std::path::Path) -> PathBuf {
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

/// 从 yt-dlp 输出中解析下载的文件路径
fn parse_download_path(output: &str, _output_dir: &PathBuf) -> Option<PathBuf> {
    for line in output.lines() {
        if line.contains("Destination:") {
            if let Some(path) = line.split("Destination:").nth(1) {
                return Some(PathBuf::from(path.trim()));
            }
        }
        if line.contains("has already been downloaded") {
            if let Some(path) = line.split("has already been downloaded").next() {
                let path = path.trim();
                if let Some(path) = path.strip_prefix("[download]") {
                    return Some(PathBuf::from(path.trim()));
                }
            }
        }
        // --print after_move:filepath 输出
        if line.starts_with('/') && !line.contains(' ') {
            let p = PathBuf::from(line.trim());
            if p.exists() {
                return Some(p);
            }
        }
    }
    None
}
