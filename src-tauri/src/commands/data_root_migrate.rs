//! 数据根整树迁移 command 薄壳。
//!
//! 业务在 ncd_runtime::data_relocate;本模块负责预检入参、停本机 Bot、
//! 写 HKCU 指针、进度事件与重启。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ncd_domain::{
    DataRootMigratePhase, DataRootMigratePreview, DataRootMigrateProgress, DataRootMigrateResult,
};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

use crate::product_registry;
use crate::AppState;

/// 前端 listen 的事件名(R3:单一字面量)。
pub const DATA_ROOT_MIGRATE_PROGRESS_EVENT: &str = "data-root-migrate-progress";

/// 进程内迁移锁:同时只允许一次;取消时置位。
///
/// `cancel` 用 Arc,与 spawn_blocking 内 execute_relocate 共用同一标志,
/// 无需第二份 AtomicBool 或轮询桥接。
pub struct DataRootMigrateGate {
    pub in_progress: AtomicBool,
    pub cancel: Arc<AtomicBool>,
    lock: Mutex<()>,
}

impl Default for DataRootMigrateGate {
    fn default() -> Self {
        Self {
            in_progress: AtomicBool::new(false),
            cancel: Arc::new(AtomicBool::new(false)),
            lock: Mutex::new(()),
        }
    }
}

impl DataRootMigrateGate {
    pub fn is_busy(&self) -> bool {
        self.in_progress.load(Ordering::SeqCst)
    }

    pub fn ensure_idle(&self) -> Result<(), String> {
        if self.is_busy() {
            Err("数据目录迁移正在进行,请稍候".into())
        } else {
            Ok(())
        }
    }

    pub async fn acquire(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.lock.lock().await
    }
}

fn emit_progress(app: &AppHandle, progress: &DataRootMigrateProgress) {
    // 与其它 DomainEvent 一致:序列化成字符串再 emit,前端 transport 会 JSON.parse
    match serde_json::to_string(progress) {
        Ok(payload) => {
            let _ = app.emit(DATA_ROOT_MIGRATE_PROGRESS_EVENT, payload);
        }
        Err(err) => {
            tracing::warn!(
                target: "ncd_tauri::data_root_migrate",
                err = %err,
                "serialize migrate progress failed"
            );
        }
    }
}

fn clear_in_progress(state: &AppState) {
    state
        .migrate_gate
        .in_progress
        .store(false, Ordering::SeqCst);
}

#[tauri::command]
pub async fn preview_migrate_data_root(
    state: State<'_, AppState>,
    target_root: String,
) -> Result<DataRootMigratePreview, String> {
    state.migrate_gate.ensure_idle()?;
    let source = state.data_root.clone();
    let target = PathBuf::from(target_root.trim());
    let local_active = state
        .bot_manager
        .count_local_active_bots()
        .await
        .map_err(|e| e.to_string())? as u32;
    Ok(ncd_runtime::preflight_relocate(
        &source,
        &target,
        local_active,
    ))
}

#[tauri::command]
pub async fn start_migrate_data_root(
    app: AppHandle,
    state: State<'_, AppState>,
    target_root: String,
) -> Result<DataRootMigrateResult, String> {
    let _guard = state.migrate_gate.acquire().await;
    state.migrate_gate.ensure_idle()?;

    let source = state.data_root.clone();
    let target = PathBuf::from(target_root.trim());
    let local_active = state
        .bot_manager
        .count_local_active_bots()
        .await
        .map_err(|e| e.to_string())? as u32;

    let preview = ncd_runtime::preflight_relocate(&source, &target, local_active);
    if !preview.ok {
        return Err(preview
            .blocking_reasons
            .first()
            .cloned()
            .unwrap_or_else(|| "预检未通过".into()));
    }

    state
        .migrate_gate
        .in_progress
        .store(true, Ordering::SeqCst);
    state.migrate_gate.cancel.store(false, Ordering::SeqCst);

    emit_progress(
        &app,
        &DataRootMigrateProgress {
            phase: DataRootMigratePhase::Freezing,
            bytes_done: 0,
            bytes_total: preview.bytes_estimate,
            current_rel: None,
            message: Some("正在停止本机 Bot…".into()),
        },
    );

    if local_active > 0 {
        let batch = state.bot_manager.shutdown_all().await;
        if !batch.failed.is_empty() {
            clear_in_progress(&state);
            return Err(format!(
                "无法停止全部本机 Bot({} 个失败),请手动停止后再迁移",
                batch.failed.len()
            ));
        }
    }

    // 进度经 channel 回 async 侧 emit,避免 spawn_blocking 闭包直接借 AppHandle
    let (progress_tx, mut progress_rx) =
        tokio::sync::mpsc::unbounded_channel::<DataRootMigrateProgress>();
    let app_for_progress = app.clone();
    let progress_pump = tokio::spawn(async move {
        while let Some(progress) = progress_rx.recv().await {
            emit_progress(&app_for_progress, &progress);
        }
    });

    // 与 cancel_migrate 共用同一 Arc<AtomicBool>
    let cancel_for_copy = Arc::clone(&state.migrate_gate.cancel);
    let result = tokio::task::spawn_blocking(move || {
        let on_progress = move |progress: DataRootMigrateProgress| {
            let _ = progress_tx.send(progress);
        };
        ncd_runtime::execute_relocate(
            &source,
            &target,
            Some(env!("CARGO_PKG_VERSION")),
            Some(cancel_for_copy.as_ref()),
            Some(&on_progress),
        )
    })
    .await
    .map_err(|e| format!("迁移任务失败: {e}"))?;

    // drop sender 在 spawn_blocking 结束后已发生;等 pump 收完
    let _ = progress_pump.await;

    let result = match result {
        Ok(r) => r,
        Err(ncd_runtime::RelocateError::Cancelled) => {
            clear_in_progress(&state);
            return Err("迁移已取消".into());
        }
        Err(err) => {
            clear_in_progress(&state);
            return Err(err.to_string());
        }
    };

    emit_progress(
        &app,
        &DataRootMigrateProgress {
            phase: DataRootMigratePhase::WritingPointer,
            bytes_done: preview.bytes_estimate,
            bytes_total: preview.bytes_estimate,
            current_rel: None,
            message: Some("正在写入数据根指针…".into()),
        },
    );

    // 无 UAC:写 HKCU
    let new_root = PathBuf::from(&result.new_root);
    if let Err(err) = product_registry::write_user_data_root(&new_root) {
        clear_in_progress(&state);
        return Err(format!(
            "数据已复制到 {},但写入用户 DataRoot 指针失败: {err}。可手动设置后重启,或删除目标目录后重试。",
            new_root.display()
        ));
    }

    emit_progress(
        &app,
        &DataRootMigrateProgress {
            phase: DataRootMigratePhase::Done,
            bytes_done: preview.bytes_estimate,
            bytes_total: preview.bytes_estimate,
            current_rel: None,
            message: Some("迁移完成,即将重启…".into()),
        },
    );

    // 重启:拉起新进程后退出当前实例
    if let Err(err) = relaunch_self() {
        tracing::error!(
            target: "ncd_tauri::data_root_migrate",
            err = %err,
            "relaunch after migrate failed; user must restart manually"
        );
        clear_in_progress(&state);
        return Ok(DataRootMigrateResult {
            restart_required: true,
            warnings: {
                let mut w = result.warnings;
                w.push(format!(
                    "自动重启失败({err}),请手动重启应用以加载新数据目录"
                ));
                w
            },
            ..result
        });
    }

    // relaunch 成功:短暂延迟后 exit,让前端收到 Done
    let app_exit = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        app_exit.exit(0);
    });

    // in_progress 保持 true 直到进程退出,阻止其它写操作
    Ok(result)
}

#[tauri::command]
pub fn cancel_migrate_data_root(state: State<'_, AppState>) -> Result<(), String> {
    if !state.migrate_gate.is_busy() {
        return Ok(());
    }
    state.migrate_gate.cancel.store(true, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub fn delete_retired_data_root(
    state: State<'_, AppState>,
    old_root: String,
) -> Result<(), String> {
    state.migrate_gate.ensure_idle()?;
    let old = PathBuf::from(old_root.trim());
    ncd_runtime::delete_retired_data_root(&old, &state.data_root)
}

fn relaunch_self() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let mut cmd = std::process::Command::new(&exe);
    // Windows:新进程组 + 分离,降低与单实例插件抢前台的干扰
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
        const DETACHED_PROCESS: u32 = 0x00000008;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
    }
    cmd.spawn()
        .map_err(|e| format!("spawn self failed: {e}"))?;
    Ok(())
}
