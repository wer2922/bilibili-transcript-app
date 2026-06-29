import { useState, useEffect, useCallback } from "react";
import {
  getTaskHistory,
  getActiveTasks,
  cancelTask,
  deleteTaskRecord,
  clearTaskHistory,
  getTaskOutputDir,
  openFolder,
  openFile,
  onTaskProgress,
  onTaskCompleted,
  type TaskRecord,
  type TaskProgressEvent,
  type TaskCompletedEvent,
} from "../lib/tauri";

interface TaskPanelProps {
  taskType: string;
  title: string;
  icon: string;
}

function TaskPanel({ taskType, title, icon }: TaskPanelProps) {
  const [tasks, setTasks] = useState<TaskRecord[]>([]);
  const [activeTasks, setActiveTasks] = useState<TaskRecord[]>([]);
  const [toast, setToast] = useState<{ message: string; type: "success" | "error" | "info" } | null>(null);

  useEffect(() => {
    if (toast) {
      const timer = setTimeout(() => setToast(null), 4000);
      return () => clearTimeout(timer);
    }
  }, [toast]);

  const refreshHistory = useCallback(async () => {
    try {
      const result = await getTaskHistory(taskType);
      setTasks(result);
    } catch {
      // 忽略
    }
  }, [taskType]);

  const refreshActive = useCallback(async () => {
    try {
      const result = await getActiveTasks();
      setActiveTasks(result.filter((t) => t.task_type === taskType));
    } catch {
      // 忽略
    }
  }, [taskType]);

  useEffect(() => {
    refreshHistory();
    refreshActive();
  }, [refreshHistory, refreshActive]);

  useEffect(() => {
    const unlistenFns: (() => void)[] = [];

    onTaskProgress((event: TaskProgressEvent) => {
      if (event.task_type === taskType) {
        setActiveTasks((prev) =>
          prev.map((t) =>
            t.id === event.task_id
              ? { ...t, progress: event.progress, speed: event.speed, eta: event.eta, status: event.status }
              : t
          )
        );
      }
    }).then((fn) => unlistenFns.push(fn));

    onTaskCompleted((event: TaskCompletedEvent) => {
      if (event.task_type === taskType) {
        setActiveTasks((prev) => prev.filter((t) => t.id !== event.task_id));
        refreshHistory();

        if (event.status === "completed") {
          setToast({ message: `✅ ${event.title || "任务"} 完成`, type: "success" });
        } else if (event.status === "failed") {
          setToast({ message: `❌ 失败: ${event.error}`, type: "error" });
        }
      }
    }).then((fn) => unlistenFns.push(fn));

    return () => unlistenFns.forEach((fn) => fn());
  }, [taskType, refreshHistory]);

  const handleCancel = async (taskId: number) => {
    try {
      await cancelTask(taskId);
      setActiveTasks((prev) => prev.filter((t) => t.id !== taskId));
      refreshHistory();
    } catch (err) {
      setToast({ message: `取消失败: ${err}`, type: "error" });
    }
  };

  const handleDelete = async (taskId: number) => {
    try {
      await deleteTaskRecord(taskId);
      setTasks((prev) => prev.filter((t) => t.id !== taskId));
    } catch (err) {
      setToast({ message: `删除失败: ${err}`, type: "error" });
    }
  };

  const handleClear = async () => {
    try {
      await clearTaskHistory(taskType);
      setTasks([]);
    } catch (err) {
      setToast({ message: `清空失败: ${err}`, type: "error" });
    }
  };

  const handleOpenFolder = async () => {
    try {
      const dir = await getTaskOutputDir(taskType);
      await openFolder(dir);
    } catch (err) {
      setToast({ message: `打开文件夹失败: ${err}`, type: "error" });
    }
  };

  const handleOpenFile = async (path: string) => {
    try {
      await openFile(path);
    } catch (err) {
      setToast({ message: `打开文件失败: ${err}`, type: "error" });
    }
  };

  const statusIcon = (status: string) => {
    switch (status) {
      case "completed": return "✅";
      case "failed": return "❌";
      case "cancelled": return "⚠️";
      case "running": return "⏳";
      case "pending": return "🕐";
      default: return "❓";
    }
  };

  const statusBadgeClass = (status: string) => {
    switch (status) {
      case "completed": return "badge-success";
      case "failed": return "badge-error";
      case "cancelled": return "badge-warning";
      default: return "badge-info";
    }
  };

  const statusText = (status: string) => {
    switch (status) {
      case "completed": return "已完成";
      case "failed": return "失败";
      case "cancelled": return "已取消";
      case "running": return "进行中";
      case "pending": return "等待中";
      default: return status;
    }
  };

  return (
    <div className="page task-page">
      {toast && (
        <div className={`toast toast-${toast.type}`}>
          <span>{toast.message}</span>
          <button className="toast-close" onClick={() => setToast(null)}>×</button>
        </div>
      )}

      <div className="task-page-header">
        <h2>{icon} {title}</h2>
        <div style={{ display: "flex", gap: 8 }}>
          <button className="btn btn-secondary btn-sm" onClick={handleOpenFolder}>
            📂 打开文件夹
          </button>
          <button className="btn btn-secondary btn-sm" onClick={handleClear}>
            🗑️ 清空历史
          </button>
        </div>
      </div>

      {activeTasks.length > 0 && (
        <div className="task-section">
          <h3 className="task-section-title">▶ 当前任务</h3>
          {activeTasks.map((task) => (
            <div key={task.id} className="task-active-card">
              <div className="task-active-info">
                <span className="task-active-title">{task.title || task.url}</span>
                <button
                  className="btn btn-secondary btn-sm"
                  onClick={() => handleCancel(task.id)}
                >
                  取消
                </button>
              </div>
              <div className="task-progress-bar-wrapper">
                <div className="task-progress-bar" style={{ width: `${task.progress}%` }} />
              </div>
              <div className="task-active-meta">
                <span>{Math.round(task.progress)}%</span>
                {task.speed && <span>{task.speed}</span>}
                {task.eta && <span>预计剩余: {task.eta}</span>}
              </div>
            </div>
          ))}
        </div>
      )}

      <div className="task-section">
        <h3 className="task-section-title">📋 历史记录</h3>
        {tasks.length === 0 ? (
          <div className="empty-state" style={{ height: 200 }}>
            <p>暂无历史记录</p>
          </div>
        ) : (
          <div className="task-history-list">
            {tasks.map((task) => (
              <div key={task.id} className="task-history-item">
                <div className="task-history-left">
                  <span className="task-history-status">{statusIcon(task.status)}</span>
                  <div className="task-history-info">
                    <span className="task-history-title">{task.title || task.url}</span>
                    <span className="task-history-meta">
                      {task.created_at}
                      {task.file_size && ` · ${task.file_size}`}
                      {task.status === "completed" && task.output_path && ` · ${task.output_path}`}
                      {task.status === "failed" && task.error && ` · ${task.error}`}
                    </span>
                  </div>
                </div>
                <div className="task-history-right">
                  <span className={`task-history-badge ${statusBadgeClass(task.status)}`}>
                    {statusText(task.status)}
                  </span>
                  {task.status === "completed" && task.output_path && (
                    <button
                      className="task-history-open"
                      onClick={() => handleOpenFile(task.output_path!)}
                      title="打开文件"
                    >
                      📂
                    </button>
                  )}
                  <button
                    className="task-history-delete"
                    onClick={() => handleDelete(task.id)}
                    title="删除"
                  >
                    ×
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="task-footer">
        共 {tasks.length} 条记录
      </div>
    </div>
  );
}

export default TaskPanel;
