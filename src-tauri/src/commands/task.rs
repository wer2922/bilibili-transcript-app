// 任务相关命令

use tauri::{command, AppHandle, State};
use tauri_plugin_shell::ShellExt;

use crate::storage::database::Database;
use crate::storage::task::TaskRecord;
use crate::task::TaskManager;

/// 获取某类型的任务历史
#[command]
pub async fn get_task_history(task_type: String) -> Result<Vec<TaskRecord>, String> {
    let db = Database::open().map_err(|e| e.to_string())?;
    crate::storage::task::get_tasks_by_type(db.conn(), &task_type)
        .map_err(|e| e.to_string())
}

/// 获取所有进行中的任务
#[command]
pub async fn get_active_tasks() -> Result<Vec<TaskRecord>, String> {
    let db = Database::open().map_err(|e| e.to_string())?;
    crate::storage::task::get_active_tasks(db.conn())
        .map_err(|e| e.to_string())
}

/// 启动视频下载任务
#[command]
pub async fn start_video_download(
    app: AppHandle,
    manager: State<'_, TaskManager>,
    url: String,
    format_id: String,
) -> Result<i64, String> {
    manager.start_video_download(app, url, format_id).await
}

/// 启动音频下载任务
#[command]
pub async fn start_audio_download(
    app: AppHandle,
    manager: State<'_, TaskManager>,
    url: String,
) -> Result<i64, String> {
    manager.start_audio_download(app, url).await
}

/// 启动转录任务
#[command]
#[allow(clippy::too_many_arguments)]
pub async fn start_transcribe(
    app: AppHandle,
    manager: State<'_, TaskManager>,
    url: String,
    language: Option<String>,
    whisper_prompt: Option<String>,
    ai_prompt: Option<String>,
    ai_context: Option<String>,
    skip_bilibili_subtitle: Option<bool>,
) -> Result<i64, String> {
    manager.start_transcribe(app, url, language, whisper_prompt, ai_prompt, ai_context, skip_bilibili_subtitle.unwrap_or(false)).await
}

/// 启动 AI 摘要任务
#[command]
pub async fn start_ai_summary(
    app: AppHandle,
    manager: State<'_, TaskManager>,
    bvid: String,
) -> Result<i64, String> {
    manager.start_ai_summary(app, bvid).await
}

/// 取消任务
#[command]
pub async fn cancel_task(
    manager: State<'_, TaskManager>,
    task_id: i64,
) -> Result<(), String> {
    manager.cancel_task(task_id).await
}

/// 删除任务记录
#[command]
pub async fn delete_task_record(task_id: i64) -> Result<(), String> {
    let db = Database::open().map_err(|e| e.to_string())?;
    crate::storage::task::delete_task(db.conn(), task_id)
        .map_err(|e| e.to_string())
}

/// 清空某类型的历史记录
#[command]
pub async fn clear_task_history(task_type: String) -> Result<(), String> {
    let db = Database::open().map_err(|e| e.to_string())?;
    crate::storage::task::clear_tasks_by_type(db.conn(), &task_type)
        .map_err(|e| e.to_string())
}

/// 获取某类型任务对应的输出目录路径
#[command]
pub async fn get_task_output_dir(task_type: String) -> Result<String, String> {
    let config = crate::config::storage::load_config().map_err(|e| e.to_string())?;
    let dir = match task_type.as_str() {
        "video_download" => &config.bilibili.video_dir,
        "audio_download" => &config.bilibili.audio_dir,
        "transcribe" => &config.bilibili.transcript_dir,
        "ai_summary" => &config.bilibili.ai_analysis_dir,
        _ => return Err("未知的任务类型".to_string()),
    };
    Ok(shellexpand::tilde(dir).to_string())
}

/// 打开文件夹
#[command]
#[allow(deprecated)]
pub async fn open_folder(app: AppHandle, path: String) -> Result<(), String> {
    // 确保目录存在
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    app.shell().open(&path, None).map_err(|e| e.to_string())?;
    Ok(())
}

/// 打开文件
#[command]
#[allow(deprecated)]
pub async fn open_file(app: AppHandle, path: String) -> Result<(), String> {
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err(format!("文件不存在: {}", path));
    }
    app.shell().open(&path, None).map_err(|e| e.to_string())?;
    Ok(())
}

/// 获取应用数据目录路径 (~/.bilibili-transcript/)
#[command]
pub async fn get_app_data_dir() -> Result<String, String> {
    let home = dirs::home_dir().ok_or("无法获取用户主目录")?;
    let data_dir = home.join(".bilibili-transcript");
    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    Ok(data_dir.to_string_lossy().to_string())
}
