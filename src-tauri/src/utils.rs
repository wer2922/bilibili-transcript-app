// 工具函数

use anyhow::{bail, Result};
use std::path::PathBuf;

/// 解析 yt-dlp 的完整路径，未找到则自动安装
///
/// App bundle 启动时 PATH 可能不包含工具路径，需要手动查找
pub fn resolve_ytdlp_path() -> Result<PathBuf> {
    resolve_or_install_tool(
        "yt-dlp",
        "YT_DLP_PATH",
        &["pip", "install", "yt-dlp"],
        &[][..],
    )
}

/// 解析 ffmpeg 的完整路径，未找到则自动安装
///
/// App bundle 启动时 PATH 可能不包含工具路径，需要手动查找
pub fn resolve_ffmpeg_path() -> Result<PathBuf> {
    resolve_or_install_tool(
        "ffmpeg",
        "FFMPEG_PATH",
        &["winget", "install", "--id", "Gyan.FFmpeg", "-e", "--silent", "--accept-source-agreements", "--accept-package-agreements"],
        &[["choco", "install", "ffmpeg", "-y"].as_slice()],
    )
}

/// 通用工具路径解析与自动安装
///
/// 查找顺序：环境变量 -> where/which -> 常见路径 -> 自动安装
fn resolve_or_install_tool(
    name: &str,
    env_var: &str,
    primary_install_cmd: &[&str],
    fallback_install_cmds: &[&[&str]],
) -> Result<PathBuf> {
    // 1. 优先使用环境变量中配置的路径
    if let Ok(path) = std::env::var(env_var) {
        let p = PathBuf::from(&path);
        if p.exists() {
            return Ok(p);
        }
    }

    // 2. 尝试 where 查找（Windows）/ which 查找（Unix）
    let find_cmd = if cfg!(target_os = "windows") { "where" } else { "which" };
    if let Ok(output) = std::process::Command::new(find_cmd).arg(name).output() {
        if output.status.success() {
            let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path_str.is_empty() {
                let p = PathBuf::from(&path_str);
                if p.exists() {
                    return Ok(p);
                }
            }
        }
    }

    // 3. 尝试常见安装路径
    #[cfg(target_os = "windows")]
    let candidates = [
        format!("C:\\ProgramData\\chocolatey\\bin\\{}.exe", name),
        format!("C:\\Users\\{}\\scoop\\shims\\{}.exe", std::env::var("USERNAME").unwrap_or_default(), name),
        format!("{}\\Programs\\Python\\Python311\\Scripts\\{}.exe", std::env::var("LOCALAPPDATA").unwrap_or_default(), name),
        format!("{}\\Programs\\Python\\Python310\\Scripts\\{}.exe", std::env::var("LOCALAPPDATA").unwrap_or_default(), name),
        format!("{}\\Python311\\Scripts\\{}.exe", std::env::var("LOCALAPPDATA").unwrap_or_default(), name),
        format!("{}\\Python310\\Scripts\\{}.exe", std::env::var("LOCALAPPDATA").unwrap_or_default(), name),
        format!("{}\\{}.exe", std::env::var("LOCALAPPDATA").unwrap_or_default(), name),
        format!("{}\\{}.exe", std::env::var("APPDATA").unwrap_or_default(), name),
        format!("C:\\ffmpeg\\bin\\{}.exe", name),
        format!("C:\\yt-dlp\\{}.exe", name),
    ];

    #[cfg(not(target_os = "windows"))]
    let candidates = [
        format!("/opt/homebrew/bin/{}", name),
        format!("/usr/local/bin/{}", name),
        format!("/usr/bin/{}", name),
        format!("/home/linuxbrew/.linuxbrew/bin/{}", name),
    ];

    for candidate in &candidates {
        let p = PathBuf::from(candidate);
        if p.exists() {
            return Ok(p);
        }
    }

    // 4. 未找到，尝试自动安装
    log::info!("未找到 {}，尝试自动安装...", name);

    let mut installed = false;

    // 尝试主安装命令
    if !primary_install_cmd.is_empty() {
        let cmd = primary_install_cmd[0];
        let args = &primary_install_cmd[1..];
        log::info!("执行安装命令: {} {:?}", cmd, args);
        if let Ok(output) = std::process::Command::new(cmd).args(args).output() {
            if output.status.success() {
                installed = true;
                log::info!("{} 安装成功", name);
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                log::warn!("{} 安装失败: {}", cmd, stderr);
            }
        }
    }

    // 尝试备用安装命令
    if !installed {
        for fallback_cmd in fallback_install_cmds {
            if fallback_cmd.is_empty() {
                continue;
            }
            let cmd = fallback_cmd[0];
            let args = &fallback_cmd[1..];
            log::info!("尝试备用安装命令: {} {:?}", cmd, args);
            if let Ok(output) = std::process::Command::new(cmd).args(args).output() {
                if output.status.success() {
                    installed = true;
                    log::info!("{} 安装成功（备用命令）", name);
                    break;
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    log::warn!("{} 安装失败: {}", cmd, stderr);
                }
            }
        }
    }

    // 安装后再次查找
    if installed {
        // 等待一下让 PATH 生效
        std::thread::sleep(std::time::Duration::from_millis(500));

        // 再次用 where/which 查找
        if let Ok(output) = std::process::Command::new(find_cmd).arg(name).output() {
            if output.status.success() {
                let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path_str.is_empty() {
                    let p = PathBuf::from(&path_str);
                    if p.exists() {
                        return Ok(p);
                    }
                }
            }
        }

        // 再次检查常见路径
        for candidate in &candidates {
            let p = PathBuf::from(candidate);
            if p.exists() {
                return Ok(p);
            }
        }
    }

    // 5. 最终失败，给出提示
    bail!(
        "未找到 {} 且自动安装失败。请手动安装后重试。\n推荐安装方式：\n  - Windows: winget install {} 或 pip install {}",
        name,
        name,
        name
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_ytdlp_path() {
        let path = resolve_ytdlp_path().expect("yt-dlp should be installed");
        assert!(path.exists(), "yt-dlp not found at {:?}", path);
    }

    #[test]
    fn test_resolve_ffmpeg_path() {
        let path = resolve_ffmpeg_path().expect("ffmpeg should be installed");
        assert!(path.exists(), "ffmpeg not found at {:?}", path);
    }
}
