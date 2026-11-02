//! 临时文件、坏文件隔离与原子替换；不依赖配置格式或历史策略。

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ncd_domain::onebot_debug::DebugStorageNotice;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

/// 临时文件留在目标同目录，启动时只清理调用方声明的文件名。
const TMP_MARKER: &str = ".tmp-";
/// Windows 杀软 / 索引服务会短暂占用刚写完的文件。
const RENAME_RETRIES: u32 = 3;
const RENAME_RETRY_DELAY: Duration = Duration::from_millis(50);

pub(super) fn strip_bom(bytes: &[u8]) -> &[u8] {
    // 记事本另存为 UTF-8 会带 BOM，手改过文件的用户不该因此丢数据
    bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)
}

/// 清掉数据目录里崩溃留下的 `<三份文件之一>.tmp-*`。尽力而为，删不掉的下次再说
pub(super) async fn remove_orphan_tmp_files(dir: &Path, files: &[&str]) {
    let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_ours = files
            .iter()
            .any(|file| name.starts_with(&format!("{file}{TMP_MARKER}")));
        if !is_ours {
            continue;
        }
        if let Err(e) = tokio::fs::remove_file(entry.path()).await {
            tracing::debug!("清理调试台遗留的临时文件失败 ({name}): {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// 挪开坏文件
// ---------------------------------------------------------------------------

/// `moved` 为 `Err` 时是没挪成的原因
pub(super) fn notice(
    file: &str,
    moved: Result<PathBuf, String>,
    reason: String,
) -> DebugStorageNotice {
    match moved {
        Ok(target) => DebugStorageNotice {
            file: file.to_owned(),
            moved_to: target.display().to_string(),
            reason,
        },
        // 没挪成（被别的进程占着之类）：moved_to 留空，原因里说清楚为什么、原件的下场
        Err(why) => DebugStorageNotice {
            file: file.to_owned(),
            moved_to: String::new(),
            reason: format!("{reason}（没能把原文件挪开: {why}；下次保存会覆盖它）"),
        },
    }
}

/// 把读不出来的文件改名成 `<名>.broken-<Unix 秒>` 留底。同一秒内已有同名时追加序号；
/// 挪不动返回原因
pub(super) async fn quarantine(path: &Path) -> Result<PathBuf, String> {
    let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
        return Err(format!("不是有效的文件路径: {}", path.display()));
    };
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    for attempt in 0..100u32 {
        let suffix = if attempt == 0 {
            stamp.to_string()
        } else {
            format!("{stamp}-{attempt}")
        };
        let target = path.with_file_name(format!("{name}.broken-{suffix}"));
        // rename 在 Unix 和 Windows 上都会直接覆盖已有的同名文件，而 `.broken-*` 是留给用户的
        // 证据，不能盖掉前一份，所以先避开已经存在的名字
        if tokio::fs::try_exists(&target).await.unwrap_or(true) {
            continue;
        }
        return match rename_with_retry(path, &target).await {
            Ok(()) => Ok(target),
            Err(e) => Err(format!("改名到 {} 失败: {e}", target.display())),
        };
    }
    Err("同一秒内的 .broken 留底重名太多".to_owned())
}

// ---------------------------------------------------------------------------
// 写盘
// ---------------------------------------------------------------------------

/// 临时文件写在目标同目录（同卷才能原子 rename）
pub(super) async fn create_tmp(target: &Path) -> Result<(PathBuf, tokio::fs::File), String> {
    let Some(name) = target.file_name() else {
        return Err(format!("不是有效的文件路径: {}", target.display()));
    };
    let mut tmp_name = name.to_os_string();
    tmp_name.push(format!("{TMP_MARKER}{}", Uuid::new_v4()));
    let tmp = target.with_file_name(tmp_name);
    let file = tokio::fs::File::create(&tmp)
        .await
        .map_err(|e| format!("创建临时文件失败 ({}): {e}", tmp.display()))?;
    Ok((tmp, file))
}

/// 刷盘并 fsync 临时文件，再改名覆盖目标。任何一步失败都清掉临时文件；
/// fsync 是为了断电后不会出现「改名已生效、内容还没落盘」的空文件
pub(super) async fn commit_tmp(
    tmp: PathBuf,
    mut file: tokio::fs::File,
    target: &Path,
) -> Result<(), String> {
    let synced = match file.flush().await {
        Ok(()) => file.sync_all().await,
        Err(e) => Err(e),
    };
    // Windows 上没关句柄就删 / 改名会失败
    drop(file);
    if let Err(e) = synced {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(format!("写入临时文件失败 ({}): {e}", tmp.display()));
    }
    if let Err(e) = rename_with_retry(&tmp, target).await {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(format!("替换 {} 失败: {e}", target.display()));
    }
    Ok(())
}

pub(super) async fn discard_tmp(tmp: &Path, file: tokio::fs::File) {
    drop(file);
    let _ = tokio::fs::remove_file(tmp).await;
}

/// rename 失败时短暂重试几次：Windows 上杀软 / 索引服务会在文件刚写完时占着它，
/// 过几十毫秒就放开了。目标不存在这类不会自己好的错误不重试
async fn rename_with_retry(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut retries = 0;
    loop {
        match tokio::fs::rename(from, to).await {
            Ok(()) => return Ok(()),
            Err(e) if retries < RENAME_RETRIES && e.kind() != ErrorKind::NotFound => {
                retries += 1;
                tokio::time::sleep(RENAME_RETRY_DELAY).await;
            }
            Err(e) => return Err(e),
        }
    }
}

pub(super) async fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let (tmp, mut file) = create_tmp(path).await?;
    if let Err(e) = file.write_all(bytes).await {
        discard_tmp(&tmp, file).await;
        return Err(format!("写入临时文件失败 ({}): {e}", tmp.display()));
    }
    commit_tmp(tmp, file, path).await
}
