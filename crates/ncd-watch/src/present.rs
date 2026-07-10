//! Desktop 在线心跳:读 state/desktop_present

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::DesktopPresentFile;

/// 判断 Desktop 是否仍视为在线
///
/// 文件缺失 / 无法解析 / 时间戳过旧 → false(watch 可接管 Webhook)
pub fn desktop_is_present(path: &Path, ttl_secs: u32) -> bool {
    if !path.is_file() {
        return false;
    }
    let updated = match read_updated_at(path) {
        Some(t) => t,
        None => return false,
    };
    let now = now_unix();
    if updated > now + 60 {
        // 时钟回拨或未来时间:保守视为在线,避免误报刷屏
        return true;
    }
    now.saturating_sub(updated) <= i64::from(ttl_secs)
}

fn read_updated_at(path: &Path) -> Option<i64> {
    let text = std::fs::read_to_string(path).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return mtime_unix(path);
    }
    if let Ok(n) = trimmed.parse::<i64>() {
        return Some(n);
    }
    if let Ok(f) = serde_json::from_str::<DesktopPresentFile>(trimmed) {
        return Some(f.updated_at_unix);
    }
    // 兼容任意 JSON 里的 updatedAtUnix
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(n) = v.get("updatedAtUnix").and_then(|x| x.as_i64()) {
            return Some(n);
        }
        if let Some(n) = v.get("updated_at_unix").and_then(|x| x.as_i64()) {
            return Some(n);
        }
    }
    mtime_unix(path)
}

fn mtime_unix(path: &Path) -> Option<i64> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    let dur = modified.duration_since(UNIX_EPOCH).ok()?;
    Some(dur.as_secs() as i64)
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn missing_file_is_absent() {
        let dir = tempdir().unwrap();
        assert!(!desktop_is_present(&dir.path().join("nope"), 90));
    }

    #[test]
    fn fresh_json_is_present() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("desktop_present");
        let body = DesktopPresentFile {
            updated_at_unix: now_unix(),
            desktop_version: Some("dev".into()),
        };
        std::fs::write(&path, serde_json::to_string(&body).unwrap()).unwrap();
        assert!(desktop_is_present(&path, 90));
    }

    #[test]
    fn stale_plain_unix_is_absent() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("desktop_present");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "{}", now_unix() - 200).unwrap();
        assert!(!desktop_is_present(&path, 90));
    }

    #[test]
    fn fresh_plain_unix_is_present() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("desktop_present");
        std::fs::write(&path, format!("{}", now_unix())).unwrap();
        assert!(desktop_is_present(&path, 90));
    }
}
