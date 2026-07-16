//! 数据根整树迁移:预检 → 复制到 staging → 校验 → promote → 重写绝对路径 → retired marker。
//!
//! 不写注册表/不重启:指针与进程生命周期由 src-tauri 负责。
//! 失败时尽量清理 staging,源根保持可继续使用。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::Local;
use ncd_domain::{
    DataRootMigratePhase, DataRootMigratePreview, DataRootMigrateProgress, DataRootMigrateResult,
    DataRootRetiredMarker, DataRootTreeEntry,
};
use tracing::{info, warn};

use crate::data_paths::DataPaths;
use crate::server_manager::ServerProfile;
use crate::server_profile_migration::migrate_server_profiles_payload;

/// staging 目录名(位于目标 data_root 内)。
pub const STAGING_DIR_NAME: &str = ".ncd-migrate-staging";

/// 复制时跳过的顶层目录名(可重建;仍会在目标建空结构由业务按需创建)。
const SKIP_TOP_LEVEL_DIRS: &[&str] = &["tmp"];

/// 校验:源存在则目标必须存在的相对路径。
const CRITICAL_REL_PATHS: &[&str] = &[
    "layout-version.json",
    "config/bot.json",
    "config/app-settings.json",
    "config/servers.json",
];

#[derive(Debug, thiserror::Error)]
pub enum RelocateError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("preflight failed: {0}")]
    Preflight(String),
    #[error("copy failed: {0}")]
    Copy(String),
    #[error("verify failed: {0}")]
    Verify(String),
    #[error("cancelled")]
    Cancelled,
    #[error("{0}")]
    Other(String),
}

pub type ProgressFn = dyn Fn(DataRootMigrateProgress) + Send + Sync;

fn emit(cb: Option<&ProgressFn>, progress: DataRootMigrateProgress) {
    if let Some(f) = cb {
        f(progress);
    }
}

fn cancelled(flag: Option<&AtomicBool>) -> bool {
    flag.map(|f| f.load(Ordering::Relaxed)).unwrap_or(false)
}

/// 规范化绝对路径(去尾部分隔)。
pub fn normalize_root(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy();
    let trimmed = raw.trim().trim_end_matches(['\\', '/']);
    if trimmed.is_empty() {
        path.to_path_buf()
    } else {
        PathBuf::from(trimmed)
    }
}

fn is_absolute_usable(path: &Path) -> bool {
    !path.as_os_str().is_empty() && path.is_absolute()
}

/// 目标是否落在源内部(含相等)。
///
/// 能 canonicalize 时用解析后路径,减轻 junction / 相对段干扰;
/// 两侧都解析失败时退回规范化字符串比较。Windows 上路径比较按不区分大小写。
pub fn target_inside_source(source: &Path, target: &Path) -> bool {
    let s = comparable_path(source);
    let t = comparable_path(target);
    if s == t {
        return true;
    }
    path_is_prefix(&s, &t)
}

fn comparable_path(path: &Path) -> PathBuf {
    let normalized = normalize_root(path);
    fs::canonicalize(&normalized).unwrap_or(normalized)
}

fn path_is_prefix(prefix: &Path, full: &Path) -> bool {
    let mut full_iter = full.components();
    for p in prefix.components() {
        match full_iter.next() {
            Some(f) if components_eq(&p, &f) => {}
            _ => return false,
        }
    }
    true
}

fn components_eq(a: &std::path::Component<'_>, b: &std::path::Component<'_>) -> bool {
    #[cfg(windows)]
    {
        match (a.as_os_str().to_str(), b.as_os_str().to_str()) {
            (Some(x), Some(y)) => x.eq_ignore_ascii_case(y),
            _ => a.as_os_str() == b.as_os_str(),
        }
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

fn dir_is_empty(path: &Path) -> io::Result<bool> {
    if !path.exists() {
        return Ok(true);
    }
    if !path.is_dir() {
        return Ok(false);
    }
    Ok(fs::read_dir(path)?.next().is_none())
}

fn should_skip_top_level(name: &str) -> bool {
    SKIP_TOP_LEVEL_DIRS
        .iter()
        .any(|s| s.eq_ignore_ascii_case(name))
        || name.eq_ignore_ascii_case(STAGING_DIR_NAME)
        || name.eq_ignore_ascii_case(DataRootRetiredMarker::FILE_NAME)
}

/// 估算将复制的字节数(与 copy 跳过规则一致)。
pub fn estimate_copy_bytes(source: &Path) -> io::Result<u64> {
    let mut total = 0u64;
    if !source.is_dir() {
        return Ok(0);
    }
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if should_skip_top_level(&name) {
            continue;
        }
        total = total.saturating_add(dir_or_file_size(&entry.path())?);
    }
    Ok(total)
}

fn dir_or_file_size(path: &Path) -> io::Result<u64> {
    let meta = fs::metadata(path)?;
    if meta.is_file() {
        return Ok(meta.len());
    }
    if !meta.is_dir() {
        return Ok(0);
    }
    let mut total = 0u64;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        total = total.saturating_add(dir_or_file_size(&entry.path())?);
    }
    Ok(total)
}

/// 预检(不写盘)。`local_active_bots` 由调用方填入。
pub fn preflight_relocate(
    source: &Path,
    target: &Path,
    local_active_bots: u32,
) -> DataRootMigratePreview {
    let source = normalize_root(source);
    let target = normalize_root(target);
    let mut blocking = Vec::new();
    let mut warnings = Vec::new();

    if !is_absolute_usable(&source) {
        blocking.push("源数据根不是可用的绝对路径".into());
    }
    if !is_absolute_usable(&target) {
        blocking.push("目标路径必须是绝对路径".into());
    }
    if source.exists() && !source.is_dir() {
        blocking.push("源数据根不是目录".into());
    }
    if !source.exists() {
        blocking.push("源数据根不存在".into());
    }
    if target_inside_source(&source, &target) {
        blocking.push("目标不能与源相同,也不能位于源目录内部".into());
    }
    if target.exists() {
        match dir_is_empty(&target) {
            Ok(true) => {}
            Ok(false) => blocking.push("目标目录必须为空(或不存在)".into()),
            Err(err) => blocking.push(format!("无法读取目标目录: {err}")),
        }
    }

    // 体积估算失败不静默当 0:提示用户,仍允许继续(可写探测兜底)
    let bytes_estimate = if source.is_dir() {
        match estimate_copy_bytes(&source) {
            Ok(n) => n,
            Err(err) => {
                warnings.push(format!("无法精确估算体积({err}),将按实际复制进度显示"));
                0
            }
        }
    } else {
        0
    };

    let tree_entries = if source.is_dir() {
        list_source_tree_entries(&source)
    } else {
        Vec::new()
    };

    if bytes_estimate > 200 * 1024 * 1024 {
        warnings.push(format!(
            "预计复制约 {:.0} MB(含组件安装树),可能需要数分钟",
            bytes_estimate as f64 / (1024.0 * 1024.0)
        ));
    }
    if local_active_bots > 0 {
        warnings.push(format!(
            "本机有 {local_active_bots} 个运行中的 Bot,开始迁移时将先停止"
        ));
    }
    warnings.push("迁移成功后需要重启应用才能使用新数据目录".into());
    warnings.push("整树包含配置、密钥、SSH 私钥与组件安装文件;请选择可信磁盘".into());
    warnings.push("旧数据目录默认保留,确认新位置正常后再手动删除".into());
    // 不链 Win32 查卷剩余:避免假硬挡;靠可写探测 + 复制期 IO 错误
    warnings.push("未检查目标卷剩余空间,请确认磁盘足够容纳整树".into());

    // 目标父目录可写探测
    if blocking.is_empty() {
        if let Some(parent) = target.parent() {
            if parent.exists() && parent.is_dir() {
                let probe = parent.join(".ncd-migrate-write-probe");
                match fs::write(&probe, b"ok") {
                    Ok(()) => {
                        let _ = fs::remove_file(&probe);
                    }
                    Err(err) => blocking.push(format!("目标父目录不可写: {err}")),
                }
            }
        }
    }

    DataRootMigratePreview {
        source_root: source.to_string_lossy().into_owned(),
        target_root: target.to_string_lossy().into_owned(),
        bytes_estimate,
        local_active_bots,
        tree_entries,
        ok: blocking.is_empty(),
        blocking_reasons: blocking,
        warnings,
    }
}

/// 源根顶层条目,供 UI 预览将复制/跳过的结构。
fn list_source_tree_entries(source: &Path) -> Vec<DataRootTreeEntry> {
    let Ok(rd) = fs::read_dir(source) else {
        return Vec::new();
    };
    let mut entries: Vec<DataRootTreeEntry> = Vec::new();
    for entry in rd.filter_map(|e| e.ok()) {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if should_skip_top_level(&name) {
            entries.push(DataRootTreeEntry {
                name,
                kind: "skip".into(),
                bytes: None,
                note: Some("不复制(可重建)".into()),
            });
            continue;
        }
        if path.is_dir() {
            let bytes = dir_or_file_size(&path).ok();
            entries.push(DataRootTreeEntry {
                name: format!("{name}/"),
                kind: "dir".into(),
                bytes,
                note: None,
            });
        } else if path.is_file() {
            let bytes = fs::metadata(&path).ok().map(|m| m.len());
            entries.push(DataRootTreeEntry {
                name,
                kind: "file".into(),
                bytes,
                note: None,
            });
        }
    }
    entries.sort_by(|a, b| {
        // 目录优先,再 skip,再文件;同组按名
        let rank = |k: &str| match k {
            "dir" => 0,
            "file" => 1,
            _ => 2,
        };
        rank(&a.kind).cmp(&rank(&b.kind)).then_with(|| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        })
    });
    entries
}

fn cleanup_staging(staging: &Path) {
    if staging.exists() {
        if let Err(err) = fs::remove_dir_all(staging) {
            warn!(
                target: "ncd_runtime::data_relocate",
                path = %staging.display(),
                err = %err,
                "failed to remove migrate staging"
            );
        }
    }
}

/// 执行整树迁移(不含注册表指针)。成功后调用方写 HKCU 并重启。
pub fn execute_relocate(
    source: &Path,
    target: &Path,
    app_version: Option<&str>,
    cancel: Option<&AtomicBool>,
    on_progress: Option<&ProgressFn>,
) -> Result<DataRootMigrateResult, RelocateError> {
    let source = normalize_root(source);
    let target = normalize_root(target);

    let preview = preflight_relocate(&source, &target, 0);
    if !preview.ok {
        return Err(RelocateError::Preflight(
            preview
                .blocking_reasons
                .first()
                .cloned()
                .unwrap_or_else(|| "preflight failed".into()),
        ));
    }

    let bytes_total = preview.bytes_estimate;
    emit(
        on_progress,
        DataRootMigrateProgress {
            phase: DataRootMigratePhase::Freezing,
            bytes_done: 0,
            bytes_total,
            current_rel: None,
            message: Some("准备复制…".into()),
        },
    );

    if cancelled(cancel) {
        return Err(RelocateError::Cancelled);
    }

    // 目标与 staging
    fs::create_dir_all(&target)?;
    let staging = target.join(STAGING_DIR_NAME);
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;

    emit(
        on_progress,
        DataRootMigrateProgress {
            phase: DataRootMigratePhase::Copying,
            bytes_done: 0,
            bytes_total,
            current_rel: None,
            message: Some("正在复制数据…".into()),
        },
    );

    let mut bytes_done = 0u64;
    // 复制/校验失败或取消:清 staging,避免目标非空导致下次预检硬挡
    if let Err(err) = copy_tree_filtered(
        &source,
        &staging,
        &source,
        cancel,
        on_progress,
        bytes_total,
        &mut bytes_done,
    ) {
        cleanup_staging(&staging);
        return Err(err);
    }

    if cancelled(cancel) {
        cleanup_staging(&staging);
        return Err(RelocateError::Cancelled);
    }

    emit(
        on_progress,
        DataRootMigrateProgress {
            phase: DataRootMigratePhase::Verifying,
            bytes_done,
            bytes_total,
            current_rel: None,
            message: Some("正在校验…".into()),
        },
    );

    if let Err(err) = verify_critical(&source, &staging) {
        cleanup_staging(&staging);
        return Err(err);
    }

    emit(
        on_progress,
        DataRootMigrateProgress {
            phase: DataRootMigratePhase::Promoting,
            bytes_done,
            bytes_total,
            current_rel: None,
            message: Some("正在提升目录…".into()),
        },
    );

    if let Err(err) = promote_staging(&staging, &target) {
        cleanup_staging(&staging);
        return Err(err);
    }

    emit(
        on_progress,
        DataRootMigrateProgress {
            phase: DataRootMigratePhase::RewritingPaths,
            bytes_done,
            bytes_total,
            current_rel: None,
            message: Some("正在重写路径引用…".into()),
        },
    );

    let mut warnings = Vec::new();
    match rewrite_server_private_key_paths(&target) {
        Ok(true) => warnings.push("已重写 servers.json 中的私钥路径".into()),
        Ok(false) => {}
        Err(err) => warnings.push(format!("重写私钥路径时警告: {err}")),
    }

    let marker_path = write_retired_marker(&source, &target, app_version)?;
    if let Some(ref p) = marker_path {
        info!(
            target: "ncd_runtime::data_relocate",
            old = %source.display(),
            new = %target.display(),
            marker = %p.display(),
            "data root relocate copy complete; pointer write is caller's job"
        );
    }

    emit(
        on_progress,
        DataRootMigrateProgress {
            phase: DataRootMigratePhase::Done,
            bytes_done,
            bytes_total,
            current_rel: None,
            message: Some("复制完成,等待写入指针并重启".into()),
        },
    );

    Ok(DataRootMigrateResult {
        old_root: source.to_string_lossy().into_owned(),
        new_root: target.to_string_lossy().into_owned(),
        retired_marker_path: marker_path.map(|p| p.to_string_lossy().into_owned()),
        restart_required: true,
        warnings,
    })
}

const PROGRESS_EMIT_EVERY_BYTES: u64 = 8 * 1024 * 1024;

fn copy_tree_filtered(
    src: &Path,
    dst: &Path,
    source_root: &Path,
    cancel: Option<&AtomicBool>,
    on_progress: Option<&ProgressFn>,
    bytes_total: u64,
    bytes_done: &mut u64,
) -> Result<(), RelocateError> {
    let mut last_emit_at = 0u64;
    copy_tree_filtered_inner(
        src,
        dst,
        source_root,
        cancel,
        on_progress,
        bytes_total,
        bytes_done,
        &mut last_emit_at,
    )
}

#[allow(clippy::too_many_arguments)]
fn copy_tree_filtered_inner(
    src: &Path,
    dst: &Path,
    source_root: &Path,
    cancel: Option<&AtomicBool>,
    on_progress: Option<&ProgressFn>,
    bytes_total: u64,
    bytes_done: &mut u64,
    last_emit_at: &mut u64,
) -> Result<(), RelocateError> {
    if cancelled(cancel) {
        return Err(RelocateError::Cancelled);
    }
    fs::create_dir_all(dst)?;

    // 顶层跳过 tmp 等可重建目录,减体积
    let is_top = src == source_root;
    let entries = fs::read_dir(src).map_err(|e| RelocateError::Copy(e.to_string()))?;
    for entry in entries {
        if cancelled(cancel) {
            return Err(RelocateError::Cancelled);
        }
        let entry = entry.map_err(|e| RelocateError::Copy(e.to_string()))?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if is_top && should_skip_top_level(&name_str) {
            continue;
        }
        let from = entry.path();
        let to = dst.join(&name);
        let ft = entry
            .file_type()
            .map_err(|e| RelocateError::Copy(e.to_string()))?;
        if ft.is_dir() {
            copy_tree_filtered_inner(
                &from,
                &to,
                source_root,
                cancel,
                on_progress,
                bytes_total,
                bytes_done,
                last_emit_at,
            )?;
        } else if ft.is_file() {
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)?;
            }
            let len = fs::copy(&from, &to)
                .map_err(|e| RelocateError::Copy(format!("copy {} failed: {e}", from.display())))?;
            *bytes_done = bytes_done.saturating_add(len);
            let should_emit = *bytes_done - *last_emit_at >= PROGRESS_EMIT_EVERY_BYTES
                || len >= PROGRESS_EMIT_EVERY_BYTES;
            if should_emit {
                *last_emit_at = *bytes_done;
                let rel = from
                    .strip_prefix(source_root)
                    .unwrap_or(&from)
                    .to_string_lossy()
                    .into_owned();
                emit(
                    on_progress,
                    DataRootMigrateProgress {
                        phase: DataRootMigratePhase::Copying,
                        bytes_done: *bytes_done,
                        bytes_total,
                        current_rel: Some(rel),
                        message: None,
                    },
                );
            }
        }
        // 忽略 symlink 等
    }
    Ok(())
}

fn verify_critical(source: &Path, staging: &Path) -> Result<(), RelocateError> {
    for rel in CRITICAL_REL_PATHS {
        let src = source.join(rel);
        if src.is_file() {
            let dst = staging.join(rel);
            if !dst.is_file() {
                return Err(RelocateError::Verify(format!("缺少关键文件: {rel}")));
            }
            // 大小一致即可(避免大文件全量 hash 拖慢)
            let s_len = fs::metadata(&src)?.len();
            let d_len = fs::metadata(&dst)?.len();
            if s_len != d_len {
                return Err(RelocateError::Verify(format!(
                    "文件大小不一致: {rel} (src={s_len} dst={d_len})"
                )));
            }
        }
    }

    // secrets / ssh_keys 目录:源有则目标有
    for dir_name in ["secrets", "ssh_keys", "components", "config", "state"] {
        let src = source.join(dir_name);
        if src.is_dir() {
            let dst = staging.join(dir_name);
            if !dst.is_dir() {
                return Err(RelocateError::Verify(format!("缺少目录: {dir_name}")));
            }
        }
    }

    // layout-version:源有则目标有;源无则不强制(空/半新环境)
    Ok(())
}

/// 将 staging 内条目移到 target 根,再删除 staging 目录。
fn promote_staging(staging: &Path, target: &Path) -> Result<(), RelocateError> {
    for entry in fs::read_dir(staging)? {
        let entry = entry?;
        let from = entry.path();
        let name = entry.file_name();
        let to = target.join(&name);
        if to.exists() {
            // 目标应为空,仅 staging 自身;若冲突则失败
            return Err(RelocateError::Other(format!(
                "promote 冲突: {} 已存在",
                to.display()
            )));
        }
        relocate_path(&from, &to)?;
    }
    fs::remove_dir_all(staging)?;
    Ok(())
}

fn relocate_path(src: &Path, dst: &Path) -> io::Result<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(_) => {
            if src.is_dir() {
                copy_dir_recursive(src, dst)?;
                fs::remove_dir_all(src)?;
            } else {
                fs::copy(src, dst)?;
                fs::remove_file(src)?;
            }
            Ok(())
        }
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn rewrite_server_private_key_paths(new_root: &Path) -> Result<bool, String> {
    let paths = DataPaths::new(new_root);
    let path = paths.servers_path();
    if !path.is_file() {
        return Ok(false);
    }
    let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut profiles: Vec<ServerProfile> = match serde_json::from_str(&text) {
        Ok(list) => list,
        Err(_) => {
            let value: serde_json::Value =
                serde_json::from_str(&text).map_err(|e| e.to_string())?;
            migrate_server_profiles_payload(value)
                .map(|r| r.profiles)
                .map_err(|e| e.to_string())?
        }
    };

    let keys_dir = paths.ssh_keys_dir();
    let mut changed = false;
    for profile in profiles.iter_mut() {
        let Some(pk) = profile.private_key_path.as_deref() else {
            continue;
        };
        let pk_path = PathBuf::from(pk);
        let Some(name) = pk_path.file_name() else {
            continue;
        };
        let new_path = keys_dir.join(name);
        if pk_path == new_path {
            continue;
        }
        // 整树已复制:新位置应有同名文件
        profile.private_key_path = Some(new_path.to_string_lossy().into_owned());
        changed = true;
    }

    if changed {
        let out = serde_json::to_string_pretty(&profiles).map_err(|e| e.to_string())?;
        fs::write(&path, out).map_err(|e| e.to_string())?;
    }
    Ok(changed)
}

fn write_retired_marker(
    old_root: &Path,
    new_root: &Path,
    app_version: Option<&str>,
) -> Result<Option<PathBuf>, RelocateError> {
    if !old_root.is_dir() {
        return Ok(None);
    }
    let marker = DataRootRetiredMarker {
        v: DataRootRetiredMarker::CURRENT_V,
        retired_at: Local::now().to_rfc3339(),
        moved_to: new_root.to_string_lossy().into_owned(),
        app_version: app_version.map(|s| s.to_string()),
    };
    let path = old_root.join(DataRootRetiredMarker::FILE_NAME);
    let text =
        serde_json::to_string_pretty(&marker).map_err(|e| RelocateError::Other(e.to_string()))?;
    match fs::write(&path, text) {
        Ok(()) => Ok(Some(path)),
        Err(err) => {
            warn!(
                target: "ncd_runtime::data_relocate",
                err = %err,
                "failed to write retired marker on old root"
            );
            Ok(None)
        }
    }
}

/// 读取旧根 retired marker(若有)。
pub fn read_retired_marker(old_root: &Path) -> Option<DataRootRetiredMarker> {
    let path = old_root.join(DataRootRetiredMarker::FILE_NAME);
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// 删除已 retired 的旧根(调用方须确认当前权威根已是 moved_to)。
pub fn delete_retired_data_root(old_root: &Path, expected_new_root: &Path) -> Result<(), String> {
    let old_root = normalize_root(old_root);
    let expected = normalize_root(expected_new_root);
    let marker = read_retired_marker(&old_root)
        .ok_or_else(|| "旧目录没有 retired 标记,拒绝删除".to_string())?;
    let moved = normalize_root(Path::new(&marker.moved_to));
    if moved != expected {
        return Err(format!(
            "retired 标记指向 {} ,与当前数据根 {} 不一致",
            moved.display(),
            expected.display()
        ));
    }
    if !old_root.is_dir() {
        return Err("旧数据目录不存在".into());
    }
    fs::remove_dir_all(&old_root).map_err(|e| format!("删除失败: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::Arc;

    fn write_file(path: &Path, body: &str) {
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).unwrap();
        }
        let mut f = fs::File::create(path).unwrap();
        f.write_all(body.as_bytes()).unwrap();
    }

    #[test]
    fn target_inside_source_detects_nested() {
        let s = PathBuf::from(r"C:\data\root");
        let t = PathBuf::from(r"C:\data\root\child");
        assert!(target_inside_source(&s, &s));
        assert!(target_inside_source(&s, &t));
        assert!(!target_inside_source(&s, Path::new(r"C:\other")));
    }

    #[test]
    fn target_inside_source_allows_sibling_prefix_names() {
        // data vs data2 不能当父子
        let s = PathBuf::from(r"C:\data");
        let sibling = PathBuf::from(r"C:\data2");
        assert!(!target_inside_source(&s, &sibling));
    }

    #[test]
    fn preflight_rejects_non_empty_target() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let dst = tmp.path().join("dst");
        fs::create_dir_all(&src).unwrap();
        write_file(&src.join("layout-version.json"), r#"{"version":1}"#);
        fs::create_dir_all(&dst).unwrap();
        write_file(&dst.join("noise.txt"), "x");
        let p = preflight_relocate(&src, &dst, 0);
        assert!(!p.ok);
        assert!(p.blocking_reasons.iter().any(|r| r.contains("空")));
    }

    #[test]
    fn execute_copies_tree_and_rewrites_key_path() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let dst = tmp.path().join("dst");
        fs::create_dir_all(src.join("config")).unwrap();
        fs::create_dir_all(src.join("ssh_keys")).unwrap();
        fs::create_dir_all(src.join("components").join("NapCatQQ")).unwrap();
        fs::create_dir_all(src.join("tmp").join("junk")).unwrap();
        write_file(&src.join("layout-version.json"), r#"{"version":1}"#);
        write_file(&src.join("config/bot.json"), r#"[]"#);
        write_file(&src.join("config/app-settings.json"), r#"{}"#);
        write_file(
            &src.join("ssh_keys/id_test"),
            "-----BEGIN PRIVATE KEY-----\nTEST\n",
        );
        // JSON 里用正斜杠,避免 Windows 反斜杠转义踩坑
        let old_key = src.join("ssh_keys").join("id_test");
        let old_key_json = old_key.to_string_lossy().replace('\\', "/");
        // ServerProfile 是 camelCase wire 格式
        let servers = format!(
            r#"[{{"id":"s1","name":"n","host":"h","port":22,"username":"u","authMethod":"key","privateKeyPath":"{old_key_json}"}}]"#
        );
        write_file(&src.join("config/servers.json"), &servers);
        write_file(
            &src.join("components/NapCatQQ/napcat.mjs"),
            "export const version = '1';",
        );
        write_file(&src.join("tmp/junk/a.txt"), "skip-me");

        let result = execute_relocate(&src, &dst, Some("3.1.2"), None, None).unwrap();
        assert_eq!(Path::new(&result.new_root), dst.as_path());
        assert!(dst.join("layout-version.json").is_file());
        assert!(dst.join("config/bot.json").is_file());
        assert!(dst.join("components/NapCatQQ/napcat.mjs").is_file());
        assert!(dst.join("ssh_keys/id_test").is_file());
        assert!(!dst.join("tmp/junk/a.txt").exists());
        assert!(!dst.join(STAGING_DIR_NAME).exists());
        let servers_text = fs::read_to_string(dst.join("config/servers.json")).unwrap();
        let profiles: Vec<ServerProfile> = serde_json::from_str(&servers_text).unwrap();
        let rewritten = profiles[0].private_key_path.as_deref().unwrap();
        assert_eq!(
            Path::new(rewritten),
            dst.join("ssh_keys").join("id_test").as_path()
        );
        assert!(src.join(DataRootRetiredMarker::FILE_NAME).is_file());
        let m = read_retired_marker(&src).unwrap();
        assert_eq!(normalize_root(Path::new(&m.moved_to)), normalize_root(&dst));
    }

    #[test]
    fn verify_fails_when_critical_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let staging = tmp.path().join("staging");
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&staging).unwrap();
        write_file(&src.join("config/bot.json"), "[]");
        let err = verify_critical(&src, &staging).unwrap_err();
        assert!(matches!(err, RelocateError::Verify(_)));
    }

    #[test]
    fn small_file_roundtrip_bytes_match() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let dst = tmp.path().join("dst");
        fs::create_dir_all(src.join("config")).unwrap();
        write_file(&src.join("layout-version.json"), r#"{"version":1}"#);
        write_file(&src.join("config/bot.json"), r#"[{"id":"b1"}]"#);
        execute_relocate(&src, &dst, None, None, None).unwrap();
        let a = fs::read(src.join("config/bot.json")).unwrap();
        let b = fs::read(dst.join("config/bot.json")).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn execute_should_remove_staging_when_cancelled_mid_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let dst = tmp.path().join("dst");
        fs::create_dir_all(src.join("config")).unwrap();
        write_file(&src.join("layout-version.json"), r#"{"version":1}"#);
        write_file(&src.join("config/bot.json"), "[]");
        for i in 0..40 {
            write_file(
                &src.join(format!("config/extra-{i}.json")),
                &format!(r#"{{"i":{i}}}"#),
            );
        }

        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_cb = Arc::clone(&cancel);
        let err = execute_relocate(
            &src,
            &dst,
            None,
            Some(cancel.as_ref()),
            Some(&move |progress| {
                if progress.phase == DataRootMigratePhase::Copying {
                    cancel_cb.store(true, Ordering::SeqCst);
                }
            }),
        )
        .unwrap_err();
        assert!(matches!(err, RelocateError::Cancelled));
        assert!(
            !dst.join(STAGING_DIR_NAME).exists(),
            "cancelled migrate must not leave staging"
        );
    }

    #[test]
    fn delete_retired_should_refuse_when_marker_mismatch() {
        let tmp = tempfile::tempdir().unwrap();
        let old = tmp.path().join("old");
        let current = tmp.path().join("current");
        fs::create_dir_all(&old).unwrap();
        fs::create_dir_all(&current).unwrap();
        write_retired_marker(&old, Path::new(r"D:\somewhere-else"), Some("1.0.0")).unwrap();
        let err = delete_retired_data_root(&old, &current).unwrap_err();
        assert!(err.contains("不一致") || err.contains("retired"));
        assert!(old.is_dir(), "must not delete on mismatch");
    }
}
