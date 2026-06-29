import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// 视频相关
export const parseVideo = (url: string) => invoke("parse_video", { url });
export const getVideoFormats = (url: string) => invoke("get_video_formats", { url });
export const downloadVideo = (url: string, formatId: string) =>
  invoke("download_video", { url, formatId });
export const downloadAudio = (url: string) => invoke("download_audio", { url });
export const fetchCover = (url: string) => invoke("fetch_cover", { url });

// 转录相关
export const transcribe = (url: string) => invoke("transcribe", { url });
export const testWhisperConnection = () => invoke("test_whisper_connection");

// 收藏夹相关
export const getFavorites = () => invoke("get_favorites");
export const getFavoriteVideos = (mediaId: string, page: number = 1) =>
  invoke("get_favorite_videos", { mediaId, page });

// 历史记录相关
export const getTranscriptHistory = () => invoke("get_transcript_history");
export const getTranscript = (bvid: string) => invoke("get_transcript", { bvid });
export const deleteTranscript = (bvid: string) => invoke("delete_transcript", { bvid });

// 配置相关
export const getConfig = () => invoke("get_config");
export const updateConfig = (config: any) => invoke("update_config", { config });

// Cookie 相关
export const importCookie = (cookie: string) => invoke("import_cookie", { cookie });
export const getCookieStatus = () => invoke("get_cookie_status");
export const importCookieFromBrowser = (browser: string) =>
  invoke("import_cookie_from_browser", { browser });
export const clearCookie = () => invoke<string>("clear_cookie");

// 日志相关
export const getRunLogs = () => invoke("get_run_logs");
export const clearRunLogs = () => invoke("clear_run_logs");
export const getLogDir = () => invoke<string>("get_log_dir");

// 任务相关
export interface TaskRecord {
  id: number;
  task_type: string;
  url: string;
  title: string;
  author: string;
  status: string;
  progress: number;
  speed: string;
  eta: string;
  output_path: string | null;
  error: string | null;
  file_size: string | null;
  created_at: string;
  updated_at: string;
  completed_at: string | null;
}

export interface TaskProgressEvent {
  task_id: number;
  task_type: string;
  status: string;
  progress: number;
  speed: string;
  eta: string;
}

export interface TaskCompletedEvent {
  task_id: number;
  task_type: string;
  status: string;
  title: string;
  output_path: string | null;
  error: string | null;
  file_size: string | null;
}

export const getTaskHistory = (taskType: string) =>
  invoke<TaskRecord[]>("get_task_history", { taskType });
export const getActiveTasks = () => invoke<TaskRecord[]>("get_active_tasks");
export const startVideoDownload = (url: string, formatId: string) =>
  invoke<number>("start_video_download", { url, formatId });
export const startAudioDownload = (url: string) =>
  invoke<number>("start_audio_download", { url });
export const startTranscribe = (
  url: string,
  language?: string,
  whisperPrompt?: string,
  aiPrompt?: string,
  aiContext?: string,
  skipBilibiliSubtitle?: boolean
) =>
  invoke<number>("start_transcribe", {
    url,
    language: language || null,
    whisperPrompt: whisperPrompt || null,
    aiPrompt: aiPrompt || null,
    aiContext: aiContext || null,
    skipBilibiliSubtitle: skipBilibiliSubtitle || false,
  });
export const startAiSummary = (bvid: string) =>
  invoke<number>("start_ai_summary", { bvid });
export const testAiSummaryConnection = () =>
  invoke<boolean>("test_ai_summary_connection");
export const cancelTask = (taskId: number) =>
  invoke<void>("cancel_task", { taskId });
export const deleteTaskRecord = (taskId: number) =>
  invoke<void>("delete_task_record", { taskId });
export const clearTaskHistory = (taskType: string) =>
  invoke<void>("clear_task_history", { taskType });
export const getTaskOutputDir = (taskType: string) =>
  invoke<string>("get_task_output_dir", { taskType });
export const openFolder = (path: string) =>
  invoke<void>("open_folder", { path });
export const openFile = (path: string) =>
  invoke<void>("open_file", { path });
export const getAppDataDir = () => invoke<string>("get_app_data_dir");

// 事件监听
export const onTaskProgress = (cb: (e: TaskProgressEvent) => void): Promise<UnlistenFn> =>
  listen<TaskProgressEvent>("task-progress", (e) => cb(e.payload));
export const onTaskCompleted = (cb: (e: TaskCompletedEvent) => void): Promise<UnlistenFn> =>
  listen<TaskCompletedEvent>("task-completed", (e) => cb(e.payload));
