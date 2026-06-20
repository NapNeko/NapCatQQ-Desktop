//! 桌面端会话日志：路径与读盘工具，对齐 legacy <data_root>/log/{timestamp}.log。
//!
//! 写入与 Tauri tracing 层在 src-tauri 接线；本模块只做路径、尾部读取与终端预览格式化。

use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// 与 legacy Logger.create_log_file 一致：<data_root>/log/。
pub fn desktop_log_dir(data_root: &Path) -> PathBuf {
    data_root.join("log")
}

/// 当前会话日志文件：目录下按修改时间取最新 .log；无则 None。
pub fn resolve_active_log_path(data_root: &Path) -> Option<PathBuf> {
    let dir = desktop_log_dir(data_root);
    if !dir.is_dir() {
        return None;
    }
    let mut best: Option<(SystemTime, PathBuf)> = None;
    for entry in fs::read_dir(&dir).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("log") {
            continue;
        }
        let modified = entry.metadata().ok()?.modified().ok()?;
        if best.as_ref().is_none_or(|(t, _)| modified > *t) {
            best = Some((modified, path));
        }
    }
    best.map(|(_, p)| p)
}

/// 删除超过 retain_days 天的 .log 文件（legacy 默认 7 天）。
pub fn purge_stale_logs(data_root: &Path, retain_days: u64) -> std::io::Result<()> {
    let dir = desktop_log_dir(data_root);
    if !dir.is_dir() {
        return Ok(());
    }
    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(retain_days * 24 * 3600))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("log") {
            continue;
        }
        if let Ok(modified) = entry.metadata()?.modified() {
            if modified < cutoff {
                let _ = fs::remove_file(&path);
            }
        }
    }
    Ok(())
}

/// 从文件尾部读取约 max_bytes 字节的 UTF-8 文本（非法字节替换）。
pub fn read_tail_text(path: &Path, max_bytes: usize) -> std::io::Result<String> {
    let mut file = fs::File::open(path)?;
    let len = file.metadata()?.len();
    let start = len.saturating_sub(max_bytes as u64);
    file.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    if start > 0 {
        // 丢弃可能被截断的首行，避免乱码行头。
        if let Some(idx) = buf.iter().position(|&b| b == b'\n') {
            buf.drain(..=idx);
        }
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// 将 legacy 六段式完整行压成设置页可读 preview（委托 ncd-log）。
pub fn format_preview_line(line: &str, inherited_level: Option<&str>) -> (Option<String>, String) {
    let raw = line.trim_end_matches('\n');
    let parts: Vec<&str> = raw.splitn(6, " | ").collect();
    if parts.len() != 6 {
        let lvl = inherited_level.map(|s| s.to_string());
        return (lvl, line.to_string());
    }
    let level_name = level_tag_name(parts[1]);
    let preview = ncd_log::preview_line(line);
    (level_name, preview)
}

fn level_tag_name(level_text: &str) -> Option<String> {
    let t = level_text.trim();
    if t.starts_with('[') && t.ends_with(']') {
        return Some(t[1..t.len() - 1].trim().to_string());
    }
    None
}

/// 按 legacy 等级名过滤预览行（EROR / WARN / INFO / DBUG / TRCE / CRIT）。
pub fn filter_preview_text(full_text: &str, level_filter: Option<&str>) -> String {
    let mut out = String::new();
    let mut current_level: Option<String> = None;
    for line in full_text.split_inclusive('\n') {
        if line.trim().is_empty() {
            continue;
        }
        let (lvl, preview) = format_preview_line(line, current_level.as_deref());
        if let Some(ref name) = lvl {
            current_level = Some(name.clone());
        }
        if let Some(filter) = level_filter {
            let active = current_level.as_deref().unwrap_or("");
            if active != filter {
                continue;
            }
        }
        out.push_str(&preview);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_preview_strips_middle_segments() {
        let raw = "26-03-22 13:42:22 | [WARN] | [ NONE_TYPE ] | [  UI  ] | [default > <qt>:0] | warn line\n";
        let (_, preview) = format_preview_line(raw, None);
        assert!(preview.contains("warn line"));
        assert!(preview.contains("[WARN]"));
    }

    #[test]
    fn filter_preview_keeps_only_matching_level() {
        let raw = concat!(
            "26-03-22 13:42:22 | [WARN] | [ X ] | [ Y ] | [Z] | warn line\n",
            "26-03-22 13:42:23 | [EROR] | [ X ] | [ Y ] | [Z] | error line\n",
            "26-03-22 13:42:24 | [INFO] | [ X ] | [ Y ] | [Z] | info line\n",
        );
        let filtered = filter_preview_text(raw, Some("EROR"));
        assert!(filtered.contains("error line"));
        assert!(!filtered.contains("warn line"));
        assert!(!filtered.contains("info line"));
    }

    #[test]
    fn resolve_active_picks_newest_log() {
        let temp = ncd_test_support::TempWorkspace::new().unwrap();
        let root = temp.path();
        let dir = desktop_log_dir(root);
        fs::create_dir_all(&dir).unwrap();
        let old = dir.join("2020-01-01_00-00-00.log");
        let new = dir.join("2026-01-01_00-00-00.log");
        fs::write(&old, b"old").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(&new, b"new").unwrap();
        let active = resolve_active_log_path(root).unwrap();
        assert_eq!(active.file_name().unwrap(), "2026-01-01_00-00-00.log");
    }
}