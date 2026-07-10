//! data_root 布局收敛:备份 → 用户数据迁到 v1 路径 → 写 marker。
//!
//! 在 MigrationOrchestrator / 业务读配置之前调用。失败时保留原目录,不半删。

use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use chrono::Local;
use tracing::{info, warn};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use crate::config_store_impl::{prune_json_bak_files, prune_migration_backups};
use crate::data_paths::{
    DataPaths, LAYOUT_VERSION, MAX_JSON_BAK_FILES, MAX_MIGRATION_BACKUPS, read_layout_version,
    write_layout_version,
};
use crate::server_manager::ServerProfile;
use crate::server_profile_migration::migrate_server_profiles_payload;

/// 桌面备份 zip 最多保留份数(按 mtime,新的优先)。
const MAX_DESKTOP_BACKUP_ZIPS: usize = 3;

/// pre-consolidate 归档最多保留份数。
const MAX_PRE_CONSOLIDATE_ARCHIVES: usize = 1;

#[derive(Debug, Clone, Default)]
pub struct ConsolidateReport {
    pub performed: bool,
    pub skipped_reason: Option<String>,
    pub backup_path: Option<PathBuf>,
    pub warnings: Vec<String>,
    pub moved: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConsolidateError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("backup failed: {0}")]
    Backup(String),
    #[error("migrate failed: {0}")]
    Migrate(String),
}

fn force_consolidate() -> bool {
    matches!(
        std::env::var("NCD_FORCE_DATA_CONSOLIDATE").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
    )
}

/// 是否需要收敛到布局 v1。
pub fn needs_consolidate(root: &Path) -> bool {
    if force_consolidate() {
        return true;
    }
    let paths = DataPaths::new(root);
    if read_layout_version(root) >= LAYOUT_VERSION {
        // 已是 v1,但仍可能残留旧 runtime 脏文件;有用户数据在旧路径且新路径缺文件时再迁
        return has_unmigrated_user_data(&paths);
    }
    // 无 marker:空目录也写 marker;有任何旧痕迹则完整收敛
    root.exists()
}

fn has_unmigrated_user_data(paths: &DataPaths) -> bool {
    let pairs = [
        (paths.legacy_bot_config_path(), paths.bot_config_path()),
        (paths.legacy_app_settings_path(), paths.app_settings_path()),
        (paths.legacy_servers_path(), paths.servers_path()),
        (
            paths.legacy_napcat_install_dir(),
            paths.napcat_install_dir(),
        ),
        (
            paths.legacy_snowluma_install_dir(),
            paths.snowluma_install_dir(),
        ),
        (paths.legacy_snowluma_data_dir(), paths.snowluma_data_dir()),
    ];
    if pairs.iter().any(|(src, dst)| {
        if src.is_file() {
            return !dst.is_file();
        }
        if src.is_dir() {
            return !dst.exists();
        }
        false
    }) {
        return true;
    }
    // 整树已迁时 config 会跟着走;若仅残留协议配置文件也要再收敛
    has_unmigrated_napcat_user_config(paths)
}

fn is_napcat_user_config_name(name: &str) -> bool {
    if !name.ends_with(".json") || name.contains(".bak.") {
        return false;
    }
    name.starts_with("onebot11_")
        || name.starts_with("napcat_")
        || name.eq_ignore_ascii_case("webui.json")
        || name.eq_ignore_ascii_case("napcat.json")
}

fn has_unmigrated_napcat_user_config(paths: &DataPaths) -> bool {
    let dst = paths.napcat_config_dir();
    for src_dir in [
        paths.legacy_napcat_install_dir().join("config"),
        paths.legacy_runtime_config_dir(),
    ] {
        if !src_dir.is_dir() {
            continue;
        }
        let Ok(entries) = fs::read_dir(&src_dir) else {
            continue;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !is_napcat_user_config_name(name) {
                continue;
            }
            if !dst.join(name).is_file() {
                return true;
            }
        }
    }
    false
}

/// 启动期入口:需要则备份并收敛;已是 v1 时仍归档 runtime/log 空壳并 GC。
pub fn consolidate_data_root(root: &Path) -> Result<ConsolidateReport, ConsolidateError> {
    let paths = DataPaths::new(root);
    if !needs_consolidate(root) {
        light_gc(&paths);
        let mut report = ConsolidateReport {
            performed: false,
            skipped_reason: Some("layout current".into()),
            ..Default::default()
        };
        // 布局已是 v1 但旧壳还在:归档到 tmp/pre-consolidate-*,不反复洗用户配置
        if has_legacy_shells(&paths) && can_archive_legacy_shells(&paths) {
            archive_legacy_shells(&paths, &mut report);
        }
        prune_desktop_backup_zips(MAX_DESKTOP_BACKUP_ZIPS);
        return Ok(report);
    }

    fs::create_dir_all(root)?;
    let mut report = ConsolidateReport {
        performed: true,
        ..Default::default()
    };

    match backup_user_data_to_desktop(root) {
        Ok(path) => {
            info!(
                target: "ncd_runtime::data_consolidate",
                backup = %path.display(),
                "data_root 用户数据已备份到桌面"
            );
            report.backup_path = Some(path);
            prune_desktop_backup_zips(MAX_DESKTOP_BACKUP_ZIPS);
        }
        Err(err) => {
            // 空目录/无用户数据时备份可跳过;有用户数据则必须成功
            if has_any_user_data(&paths) {
                return Err(err);
            }
            report.warnings.push(format!("backup skipped: {err}"));
        }
    }

    migrate_in_place(&paths, &mut report)?;
    light_gc(&paths);
    write_layout_version(root, LAYOUT_VERSION)?;

    // 用户数据已在新路径后,把 runtime/ 与旧 log/ 挪到 pre-consolidate 归档
    if can_archive_legacy_shells(&paths) {
        archive_legacy_shells(&paths, &mut report);
    } else {
        report
            .warnings
            .push("legacy shells kept: safety check failed".into());
    }

    info!(
        target: "ncd_runtime::data_consolidate",
        moved = report.moved.len(),
        warnings = report.warnings.len(),
        "data_root 已收敛到 layout v1"
    );
    Ok(report)
}

fn has_any_user_data(paths: &DataPaths) -> bool {
    [
        paths.bot_config_path(),
        paths.legacy_bot_config_path(),
        paths.app_settings_path(),
        paths.legacy_app_settings_path(),
        paths.servers_path(),
        paths.legacy_servers_path(),
        paths.legacy_app_config_path(),
        paths.app_config_path(),
    ]
    .iter()
    .any(|p| p.is_file())
        || paths.secrets_dir().is_dir()
        || paths.ssh_keys_dir().is_dir()
        || paths.legacy_snowluma_data_dir().is_dir()
}

fn has_legacy_shells(paths: &DataPaths) -> bool {
    paths.legacy_runtime_dir().exists() || paths.legacy_desktop_log_dir().is_dir()
}

/// 仅当新布局已接住关键用户数据时才允许挪走旧壳。
fn can_archive_legacy_shells(paths: &DataPaths) -> bool {
    if read_layout_version(paths.root()) < LAYOUT_VERSION {
        return false;
    }
    if has_unmigrated_user_data(paths) {
        return false;
    }
    // 旧安装树还在且新路径没有 → 不能归档
    if paths.legacy_napcat_install_dir().is_dir() && !paths.napcat_install_dir().exists() {
        return false;
    }
    if paths.legacy_snowluma_install_dir().is_dir() && !paths.snowluma_install_dir().exists() {
        return false;
    }
    // 旧 bot 配置还在时,新路径必须已有
    if paths.legacy_bot_config_path().is_file() && !paths.bot_config_path().is_file() {
        return false;
    }
    true
}

fn archive_legacy_shells(paths: &DataPaths, report: &mut ConsolidateReport) {
    // 归档前尽量把 bot 日志补进 logs/bots(目标已存在时 move_dir 会跳过)
    merge_bot_logs_if_needed(paths, report);

    let stamp = Local::now().format("%Y%m%d-%H%M%S");
    let archive_root = paths.tmp_dir().join(format!("pre-consolidate-{stamp}"));
    if let Err(err) = fs::create_dir_all(&archive_root) {
        report
            .warnings
            .push(format!("pre-consolidate mkdir failed: {err}"));
        return;
    }

    let mut archived_any = false;
    for (src, name) in [
        (paths.legacy_runtime_dir(), "runtime"),
        (paths.legacy_desktop_log_dir(), "log"),
    ] {
        if !src.exists() {
            continue;
        }
        let dst = archive_root.join(name);
        match relocate_path(&src, &dst) {
            Ok(()) => {
                report
                    .moved
                    .push(format!("archived {name} -> {}", dst.display()));
                archived_any = true;
            }
            Err(err) => {
                report
                    .warnings
                    .push(format!("archive {name} failed: {err}"));
            }
        }
    }

    if archived_any {
        info!(
            target: "ncd_runtime::data_consolidate",
            archive = %archive_root.display(),
            "legacy runtime/log 已归档到 pre-consolidate"
        );
    } else {
        // 空归档目录清掉
        let _ = fs::remove_dir_all(&archive_root);
    }

    prune_pre_consolidate_archives(&paths.tmp_dir(), MAX_PRE_CONSOLIDATE_ARCHIVES);
}

fn merge_bot_logs_if_needed(paths: &DataPaths, report: &mut ConsolidateReport) {
    let src = paths.legacy_bot_log_dir();
    let dst = paths.bot_log_dir();
    if !src.is_dir() {
        return;
    }
    if !dst.exists() {
        let _ = move_dir_prefer(&src, &dst, false, "logs/bots", report);
        return;
    }
    // 两边都在:把源里缺的文件拷过去,不覆盖
    let Ok(entries) = fs::read_dir(&src) else {
        return;
    };
    let mut n = 0u32;
    for entry in entries.filter_map(|e| e.ok()) {
        let from = entry.path();
        if !from.is_file() {
            continue;
        }
        let name = entry.file_name();
        let to = dst.join(&name);
        if to.exists() {
            continue;
        }
        if fs::copy(&from, &to).is_ok() {
            n += 1;
        }
    }
    if n > 0 {
        report.moved.push(format!("logs/bots merged {n} files"));
    }
}

fn relocate_path(src: &Path, dst: &Path) -> std::io::Result<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(_) => {
            // 跨卷 rename 失败:递归复制后删源
            copy_dir_recursive(src, dst)?;
            fs::remove_dir_all(src)?;
            Ok(())
        }
    }
}

fn prune_pre_consolidate_archives(tmp_dir: &Path, keep: usize) {
    if !tmp_dir.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(tmp_dir) else {
        return;
    };
    let mut dirs: Vec<(PathBuf, std::time::SystemTime)> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("pre-consolidate-"))
        })
        .map(|p| {
            let mtime = fs::metadata(&p)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            (p, mtime)
        })
        .collect();
    dirs.sort_by(|a, b| b.1.cmp(&a.1));
    for (path, _) in dirs.into_iter().skip(keep) {
        let _ = fs::remove_dir_all(path);
    }
}

fn prune_desktop_backup_zips(keep: usize) {
    let Some(desktop) = dirs::desktop_dir().or_else(dirs::home_dir) else {
        return;
    };
    let Ok(entries) = fs::read_dir(&desktop) else {
        return;
    };
    let mut zips: Vec<(PathBuf, std::time::SystemTime)> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                    n.starts_with("NapCatQQ-Desktop-backup-2x-") && n.ends_with(".zip")
                })
        })
        .map(|p| {
            let mtime = fs::metadata(&p)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            (p, mtime)
        })
        .collect();
    if zips.len() <= keep {
        return;
    }
    zips.sort_by(|a, b| b.1.cmp(&a.1));
    for (path, _) in zips.into_iter().skip(keep) {
        if let Err(err) = fs::remove_file(&path) {
            warn!(
                target: "ncd_runtime::data_consolidate",
                path = %path.display(),
                err = %err,
                "prune desktop backup zip failed"
            );
        }
    }
}

fn light_gc(paths: &DataPaths) {
    for dir in [
        paths.config_dir(),
        paths.legacy_runtime_config_dir(),
        paths.napcat_config_dir(),
        paths.legacy_napcat_install_dir().join("config"),
    ] {
        if !dir.is_dir() {
            continue;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let p = entry.path();
            if !p.is_file() {
                continue;
            }
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.contains(".bak.") {
                // 按逻辑文件名 prune:取 `.bak.` 前缀
                if let Some(idx) = name.find(".bak.") {
                    let logical = dir.join(&name[..idx]);
                    prune_json_bak_files(&logical, MAX_JSON_BAK_FILES);
                }
            }
        }
    }
    prune_migration_backups(&paths.migration_backup_dir(), MAX_MIGRATION_BACKUPS);
    prune_migration_backups(&paths.legacy_migration_backup_dir(), MAX_MIGRATION_BACKUPS);
}

fn migrate_in_place(
    paths: &DataPaths,
    report: &mut ConsolidateReport,
) -> Result<(), ConsolidateError> {
    ensure_layout_dirs(paths)?;

    // 配置文件:旧 → 新(新已存在则不覆盖,除非 force)
    let force = force_consolidate();
    copy_file_prefer(
        &paths.legacy_bot_config_path(),
        &paths.bot_config_path(),
        force,
        "bot.json",
        report,
    )?;
    copy_file_prefer(
        &paths.legacy_app_settings_path(),
        &paths.app_settings_path(),
        force,
        "app-settings.json",
        report,
    )?;
    // servers:优先已有 config/servers.json;否则从 runtime/config 迁
    if !paths.servers_path().is_file() || force {
        copy_file_prefer(
            &paths.legacy_servers_path(),
            &paths.servers_path(),
            force,
            "servers.json",
            report,
        )?;
    }
    // 旧 QConfig 保留副本到 config/config.json 供 migration 抽字段
    copy_file_prefer(
        &paths.legacy_app_config_path(),
        &paths.app_config_path(),
        force,
        "config.json",
        report,
    )?;
    copy_file_prefer(
        &paths
            .legacy_runtime_config_dir()
            .join("migration-report.json"),
        &paths.migration_report_path(),
        force,
        "migration-report.json",
        report,
    )?;

    // 组件安装树
    move_dir_prefer(
        &paths.legacy_napcat_install_dir(),
        &paths.napcat_install_dir(),
        force,
        "components/NapCatQQ",
        report,
    )?;
    move_dir_prefer(
        &paths.legacy_snowluma_install_dir(),
        &paths.snowluma_install_dir(),
        force,
        "components/SnowLuma",
        report,
    )?;

    // SnowLuma 数据
    move_dir_prefer(
        &paths.legacy_snowluma_data_dir(),
        &paths.snowluma_data_dir(),
        force,
        "state/snowluma",
        report,
    )?;

    // Bot 日志
    move_dir_prefer(
        &paths.legacy_bot_log_dir(),
        &paths.bot_log_dir(),
        force,
        "logs/bots",
        report,
    )?;

    // 桌面日志:目录占位;历史会话 log 进 pre-consolidate 归档
    if paths.legacy_desktop_log_dir().is_dir() && !paths.desktop_log_dir().exists() {
        let _ = fs::create_dir_all(paths.desktop_log_dir());
        report.moved.push("logs/desktop (dir created)".into());
    }

    // NapCat 用户协议配置:整树 move 已带走时 no-op;仅 config 残留或装在新路径时补拷
    migrate_napcat_user_config(paths, force, report)?;

    rewrite_server_private_key_paths(paths, report)?;

    // 清理旧 runtime/config 下 bak 与空 migration-backup 噪音
    cleanup_legacy_noise(paths, report);

    Ok(())
}

fn ensure_layout_dirs(paths: &DataPaths) -> std::io::Result<()> {
    for dir in [
        paths.config_dir(),
        paths.secrets_dir(),
        paths.ssh_keys_dir(),
        paths.components_dir(),
        paths.state_dir(),
        paths.logs_dir(),
        paths.desktop_log_dir(),
        paths.bot_log_dir(),
        paths.cache_dir(),
        paths.tmp_dir(),
        paths.output_dir(),
    ] {
        fs::create_dir_all(dir)?;
    }
    Ok(())
}

fn copy_file_prefer(
    src: &Path,
    dst: &Path,
    force: bool,
    label: &str,
    report: &mut ConsolidateReport,
) -> Result<(), ConsolidateError> {
    if !src.is_file() {
        return Ok(());
    }
    if dst.is_file() && !force {
        return Ok(());
    }
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(src, dst).map_err(|e| ConsolidateError::Migrate(format!("copy {label}: {e}")))?;
    report.moved.push(label.to_string());
    Ok(())
}

/// 迁 onebot11_*/napcat_*/webui 等到 components/NapCatQQ/config。
fn migrate_napcat_user_config(
    paths: &DataPaths,
    force: bool,
    report: &mut ConsolidateReport,
) -> Result<(), ConsolidateError> {
    let dst_dir = paths.napcat_config_dir();
    let mut copied = 0u32;
    for src_dir in [
        paths.legacy_napcat_install_dir().join("config"),
        paths.legacy_runtime_config_dir(),
    ] {
        if !src_dir.is_dir() {
            continue;
        }
        let Ok(entries) = fs::read_dir(&src_dir) else {
            continue;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let src = entry.path();
            if !src.is_file() {
                continue;
            }
            let name = src
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            if !is_napcat_user_config_name(&name) {
                continue;
            }
            let dst = dst_dir.join(&name);
            let before = report.moved.len();
            copy_file_prefer(
                &src,
                &dst,
                force,
                &format!("components/NapCatQQ/config/{name}"),
                report,
            )?;
            if report.moved.len() > before {
                copied += 1;
            }
        }
    }
    if copied > 0 {
        info!(
            target: "ncd_runtime::data_consolidate",
            copied,
            "NapCat 用户协议配置已迁入 components/NapCatQQ/config"
        );
    }
    Ok(())
}

fn move_dir_prefer(
    src: &Path,
    dst: &Path,
    force: bool,
    label: &str,
    report: &mut ConsolidateReport,
) -> Result<(), ConsolidateError> {
    if !src.is_dir() {
        return Ok(());
    }
    if dst.exists() {
        if !force {
            return Ok(());
        }
        // force 时若目标已存在,不覆盖大目录,只记警告
        report
            .warnings
            .push(format!("{label}: destination exists, skip move"));
        return Ok(());
    }
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::rename(src, dst) {
        Ok(()) => {
            report.moved.push(label.to_string());
            Ok(())
        }
        Err(_) => {
            // 跨卷 rename 失败则递归复制后删源
            copy_dir_recursive(src, dst)
                .map_err(|e| ConsolidateError::Migrate(format!("copy dir {label}: {e}")))?;
            let _ = fs::remove_dir_all(src);
            report.moved.push(format!("{label} (copied)"));
            Ok(())
        }
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn rewrite_server_private_key_paths(
    paths: &DataPaths,
    report: &mut ConsolidateReport,
) -> Result<(), ConsolidateError> {
    let path = paths.servers_path();
    if !path.is_file() {
        return Ok(());
    }
    let text = fs::read_to_string(&path)?;

    // 优先强类型 Vec<ServerProfile>;旧字段名走 migration 再取 profiles
    // (临时:完整 servers 读写仍由 ServerManager 负责,此处只改 private_key_path)
    let mut profiles: Vec<ServerProfile> = match serde_json::from_str(&text) {
        Ok(list) => list,
        Err(_) => {
            let Ok(value) = serde_json::from_str(&text) else {
                report
                    .warnings
                    .push("servers.json parse failed, skip key path rewrite".into());
                return Ok(());
            };
            match migrate_server_profiles_payload(value) {
                Ok(result) => result.profiles,
                Err(err) => {
                    report.warnings.push(format!(
                        "servers.json profile migrate failed, skip key path rewrite: {err}"
                    ));
                    return Ok(());
                }
            }
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
        // 若旧路径文件还在且新位置没有,拷过去
        if pk_path.is_file() && !new_path.is_file() {
            let _ = fs::create_dir_all(&keys_dir);
            let _ = fs::copy(&pk_path, &new_path);
        }
        profile.private_key_path = Some(new_path.to_string_lossy().into_owned());
        changed = true;
    }

    if changed {
        let out = serde_json::to_string_pretty(&profiles)
            .map_err(|e| ConsolidateError::Migrate(e.to_string()))?;
        fs::write(&path, out)?;
        report
            .moved
            .push("servers.json private_key_path rewrite".into());
    }
    Ok(())
}

fn cleanup_legacy_noise(paths: &DataPaths, report: &mut ConsolidateReport) {
    // 删除旧 config 目录下全部 .bak.*
    for dir in [paths.legacy_runtime_config_dir(), paths.config_dir()] {
        if !dir.is_dir() {
            continue;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let p = entry.path();
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.contains(".bak.") && p.is_file() {
                let _ = fs::remove_file(&p);
            }
        }
    }
    // 旧 migration-backup 整树可删(已有桌面 zip)
    let legacy_bak = paths.legacy_migration_backup_dir();
    if legacy_bak.is_dir() {
        let _ = fs::remove_dir_all(&legacy_bak);
        report.moved.push("removed legacy migration-backup".into());
    }
}

fn backup_user_data_to_desktop(root: &Path) -> Result<PathBuf, ConsolidateError> {
    let desktop = dirs::desktop_dir()
        .or_else(dirs::home_dir)
        .ok_or_else(|| ConsolidateError::Backup("cannot resolve desktop/home".into()))?;
    // 秒级 stamp + 纳秒,避免并行测试/同秒二次 force 撞名
    let stamp = Local::now().format("%Y%m%d-%H%M%S");
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let zip_path = desktop.join(format!("NapCatQQ-Desktop-backup-2x-{stamp}-{nanos}.zip"));

    let file = fs::File::create(&zip_path).map_err(|e| ConsolidateError::Backup(e.to_string()))?;
    let mut zip = ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let paths = DataPaths::new(root);
    let mut files: Vec<(PathBuf, String)> = Vec::new();

    // 新/旧路径用不同 zip 条目名,避免 Duplicate filename
    let owned: Vec<(PathBuf, String)> = vec![
        (paths.bot_config_path(), "config/bot.json".into()),
        (
            paths.legacy_bot_config_path(),
            "legacy-runtime-config/bot.json".into(),
        ),
        (paths.app_settings_path(), "config/app-settings.json".into()),
        (
            paths.legacy_app_settings_path(),
            "legacy-runtime-config/app-settings.json".into(),
        ),
        (paths.app_config_path(), "config/config.json".into()),
        (
            paths.legacy_app_config_path(),
            "legacy-runtime-config/config.json".into(),
        ),
        (paths.servers_path(), "config/servers.json".into()),
        (
            paths.legacy_servers_path(),
            "legacy-runtime-config/servers.json".into(),
        ),
        (
            paths.migration_report_path(),
            "config/migration-report.json".into(),
        ),
        (
            paths
                .legacy_runtime_config_dir()
                .join("migration-report.json"),
            "legacy-runtime-config/migration-report.json".into(),
        ),
    ];
    for (p, name) in owned {
        if p.is_file() {
            files.push((p, name));
        }
    }

    collect_files_under(&paths.secrets_dir(), "secrets", &mut files);
    collect_files_under(&paths.ssh_keys_dir(), "ssh_keys", &mut files);
    collect_files_under(&paths.legacy_snowluma_data_dir(), "snowluma", &mut files);
    collect_files_under(&paths.snowluma_data_dir(), "state/snowluma", &mut files);
    collect_files_under(
        &paths.legacy_napcat_install_dir().join("config"),
        "runtime-NapCatQQ-config",
        &mut files,
    );
    collect_files_under(
        &paths.napcat_config_dir(),
        "components-NapCatQQ-config",
        &mut files,
    );

    // 目录收集也可能撞名;按 zip 条目名去重(保留先出现的)
    {
        let mut dedup = Vec::new();
        let mut names = HashSet::new();
        for (p, name) in files {
            if names.insert(name.clone()) {
                dedup.push((p, name));
            }
        }
        files = dedup;
    }

    if files.is_empty() {
        zip.start_file("README.txt", opts)
            .map_err(|e| ConsolidateError::Backup(e.to_string()))?;
        zip.write_all(b"NapCatQQ Desktop data backup (no user config files found)\n")
            .map_err(|e| ConsolidateError::Backup(e.to_string()))?;
    } else {
        for (abs, name) in files {
            let mut buf = Vec::new();
            let mut f =
                fs::File::open(&abs).map_err(|e| ConsolidateError::Backup(e.to_string()))?;
            f.read_to_end(&mut buf)
                .map_err(|e| ConsolidateError::Backup(e.to_string()))?;
            zip.start_file(name, opts)
                .map_err(|e| ConsolidateError::Backup(e.to_string()))?;
            zip.write_all(&buf)
                .map_err(|e| ConsolidateError::Backup(e.to_string()))?;
        }
    }

    let meta = format!(
        "source={}\ncreated_at={}\nlayout_target={}\n",
        root.display(),
        Local::now().to_rfc3339(),
        LAYOUT_VERSION
    );
    zip.start_file("backup_meta.txt", opts)
        .map_err(|e| ConsolidateError::Backup(e.to_string()))?;
    zip.write_all(meta.as_bytes())
        .map_err(|e| ConsolidateError::Backup(e.to_string()))?;
    zip.finish()
        .map_err(|e| ConsolidateError::Backup(e.to_string()))?;

    Ok(zip_path)
}

fn collect_files_under(dir: &Path, prefix: &str, out: &mut Vec<(PathBuf, String)>) {
    if !dir.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file")
                .to_string();
            out.push((path, format!("{prefix}/{name}")));
        } else if path.is_dir() {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("dir")
                .to_string();
            collect_files_under(&path, &format!("{prefix}/{name}"), out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn consolidates_legacy_runtime_config_to_layout_v1() {
        let temp = ncd_test_support::TempWorkspace::new().unwrap();
        let root = temp.path();
        let paths = DataPaths::new(root);
        // 无 force 时:legacy 有数据且新路径缺文件也会触发
        fs::create_dir_all(paths.legacy_runtime_config_dir()).unwrap();
        fs::write(
            paths.legacy_bot_config_path(),
            serde_json::to_vec_pretty(&json!({
                "info": {"configVersion": "v2.1"},
                "bots": []
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            paths.legacy_app_settings_path(),
            serde_json::to_vec_pretty(&json!({"closeAction": "exit"})).unwrap(),
        )
        .unwrap();
        fs::create_dir_all(paths.legacy_napcat_install_dir().join("config")).unwrap();
        fs::write(
            paths
                .legacy_napcat_install_dir()
                .join("config")
                .join("webui.json"),
            b"{}",
        )
        .unwrap();

        let report = consolidate_data_root(root).expect("consolidate");

        assert!(report.performed);
        assert!(paths.bot_config_path().is_file());
        assert!(paths.app_settings_path().is_file());
        assert!(
            paths
                .napcat_install_dir()
                .join("config")
                .join("webui.json")
                .is_file()
        );
        assert_eq!(read_layout_version(root), LAYOUT_VERSION);
        // runtime 壳应被归档走
        assert!(!paths.legacy_runtime_dir().exists());
        let archives: Vec<_> = fs::read_dir(paths.tmp_dir())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("pre-consolidate-"))
            })
            .collect();
        assert_eq!(archives.len(), 1);

        // 二次调用应跳过(无 unmigrated / 无壳)
        let report2 = consolidate_data_root(root).expect("second");
        assert!(!report2.performed);
    }

    #[test]
    fn migrates_napcat_user_config_when_install_already_at_new_path() {
        let temp = ncd_test_support::TempWorkspace::new().unwrap();
        let root = temp.path();
        let paths = DataPaths::new(root);
        write_layout_version(root, LAYOUT_VERSION).unwrap();
        // 新路径已有安装树,旧路径只剩协议配置
        fs::create_dir_all(paths.napcat_install_dir()).unwrap();
        fs::write(paths.napcat_install_dir().join("napcat.mjs"), b"// v").unwrap();
        fs::create_dir_all(paths.legacy_napcat_install_dir().join("config")).unwrap();
        fs::write(
            paths
                .legacy_napcat_install_dir()
                .join("config")
                .join("onebot11_10001.json"),
            br#"{"network":{}}"#,
        )
        .unwrap();
        fs::create_dir_all(paths.legacy_runtime_config_dir()).unwrap();
        fs::write(
            paths.legacy_runtime_config_dir().join("napcat_10001.json"),
            br#"{}"#,
        )
        .unwrap();

        let report = consolidate_data_root(root).expect("consolidate napcat config");
        assert!(report.performed);
        assert!(
            paths
                .napcat_config_dir()
                .join("onebot11_10001.json")
                .is_file()
        );
        assert!(
            paths
                .napcat_config_dir()
                .join("napcat_10001.json")
                .is_file()
        );
    }

    #[test]
    fn archives_leftover_runtime_shell_when_layout_already_v1() {
        let temp = ncd_test_support::TempWorkspace::new().unwrap();
        let root = temp.path();
        let paths = DataPaths::new(root);
        write_layout_version(root, LAYOUT_VERSION).unwrap();
        fs::create_dir_all(paths.config_dir()).unwrap();
        fs::write(paths.bot_config_path(), br#"{"bots":[]}"#).unwrap();
        fs::create_dir_all(paths.legacy_runtime_config_dir()).unwrap();
        fs::write(paths.legacy_bot_config_path(), br#"{"bots":[]}"#).unwrap();
        fs::create_dir_all(paths.legacy_desktop_log_dir()).unwrap();
        fs::write(paths.legacy_desktop_log_dir().join("old.log"), b"x").unwrap();

        let report = consolidate_data_root(root).expect("archive shells");
        assert!(!report.performed);
        assert!(!paths.legacy_runtime_dir().exists());
        assert!(!paths.legacy_desktop_log_dir().exists());
        assert!(report.moved.iter().any(|m| m.contains("archived")));
    }
}
