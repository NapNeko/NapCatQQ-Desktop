use std::path::PathBuf;
use std::process::Command;

use ncd_runtime::{desktop_log, LogSnapshot};
use serde::Deserialize;
use tauri::State;

use crate::AppState;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DesktopLogLevelFilter {
    /// legacy 等级名:EROR / WARN / INFO / DBUG / TRCE / CRIT;空或省略表示全部
    pub level: Option<String>,
}

#[tauri::command]
pub fn tail_desktop_log(
    state: State<'_, AppState>,
    lines: Option<usize>,
    level_filter: Option<DesktopLogLevelFilter>,
) -> Result<LogSnapshot, String> {
    let path = crate::desktop_log::active_log_path()
        .or_else(|| desktop_log::resolve_active_log_path(&state.data_root))
        .ok_or_else(|| "当前会话尚未创建 Desktop 日志文件".to_string())?;

    let max_bytes = lines.unwrap_or(800).saturating_mul(512).max(64 * 1024);
    let raw = desktop_log::read_tail_text(&path, max_bytes)
        .map_err(|err| format!("读取 Desktop 日志失败: {err}"))?;

    let filter = level_filter
        .and_then(|f| f.level)
        .filter(|s| !s.is_empty() && s != "ALL_");
    let text = desktop_log::filter_preview_text(&raw, filter.as_deref());

    let mut line_vec: Vec<String> = if text.is_empty() {
        Vec::new()
    } else {
        text.split_inclusive('\n')
            .map(|s| s.to_string())
            .collect()
    };
    let take = lines.unwrap_or(800);
    if line_vec.len() > take {
        line_vec = line_vec.split_off(line_vec.len() - take);
    }
    let total = line_vec.len();
    Ok(LogSnapshot {
        lines: line_vec,
        total_lines: total,
    })
}

#[tauri::command]
pub fn open_desktop_log_location(state: State<'_, AppState>) -> Result<PathBuf, String> {
    let path = crate::desktop_log::active_log_path()
        .or_else(|| desktop_log::resolve_active_log_path(&state.data_root))
        .ok_or_else(|| "当前会话尚未创建 Desktop 日志文件".to_string())?;

    open_in_file_manager_select(&path)?;
    Ok(path)
}

fn open_in_file_manager_select(path: &std::path::Path) -> Result<(), String> {
    let status = if cfg!(target_os = "windows") {
        let arg = format!("/select,{}", path.display());
        Command::new("explorer").arg(arg).status()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg("-R").arg(path).status()
    } else {
        if let Some(parent) = path.parent() {
            Command::new("xdg-open").arg(parent).status()
        } else {
            return Err("无法解析日志文件所在目录".to_string());
        }
    }
    .map_err(|err| format!("打开日志位置失败: {err}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("文件管理器退出失败: {status}"))
    }
}