// Bilibili Transcript App - Tauri 入口
// Windows 原生应用，支持 B站视频转录、字幕获取、Whisper 语音转文字

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(dead_code)]

mod bilibili;
mod commands;
mod config;
mod download;
mod storage;
mod summary;
mod task;
mod transcribe;
mod utils;

use config::AppConfig;

fn main() {
    commands::log::init_logger();
    log::info!("Bilibili Transcript 启动中...");
    log::info!("版本: {} ({})", env!("CARGO_PKG_VERSION"), if cfg!(debug_assertions) { "debug" } else { "release" });

    let app_config = config::storage::load_config().unwrap_or_else(|e| {
        log::warn!("加载配置失败，使用默认配置: {}", e);
        AppConfig::default()
    });

    log::info!("配置加载完成");

    // 清理上次运行残留的 running 状态任务（标记为 failed）
    if let Ok(db) = storage::database::Database::open() {
        if let Err(e) = storage::task::cleanup_stale_tasks(db.conn()) {
            log::warn!("清理残留任务失败: {}", e);
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(app_config)
        .manage(task::TaskManager::new())
        .invoke_handler(tauri::generate_handler![
            commands::video::parse_video,
            commands::video::get_video_formats,
            commands::video::download_video,
            commands::video::download_audio,
            commands::video::fetch_cover,
            commands::transcribe::transcribe,
            commands::transcribe::test_whisper_connection,
            commands::transcribe::test_ai_summary_connection,
            commands::favorite::get_favorites,
            commands::favorite::get_favorite_videos,
            commands::history::get_transcript_history,
            commands::history::get_transcript,
            commands::history::delete_transcript,
            commands::config::get_config,
            commands::config::update_config,
            commands::cookie::import_cookie,
            commands::cookie::get_cookie_status,
            commands::cookie::import_cookie_from_browser,
            commands::cookie::clear_cookie,
            commands::log::get_run_logs,
            commands::log::clear_run_logs,
            commands::log::get_log_dir,
            commands::task::get_task_history,
            commands::task::get_active_tasks,
            commands::task::start_video_download,
            commands::task::start_audio_download,
            commands::task::start_transcribe,
            commands::task::start_ai_summary,
            commands::task::cancel_task,
            commands::task::delete_task_record,
            commands::task::clear_task_history,
            commands::task::get_task_output_dir,
            commands::task::open_folder,
            commands::task::open_file,
            commands::task::get_app_data_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
