//! 框架配置恢复目标预检与持久化待恢复检查点。

use super::*;
use ncd_appframework::config_backup::{
    FrameworkConfigBackup, FrameworkConfigRecovery, FrameworkConfigRestorePlan,
    capture_config_backup, checked_install_root, safe_instance_id,
};
use ncd_host::SshDialTarget;
use ncd_server::ServerProfile;

pub struct PreparedFrameworkRestore {
    pub plan: FrameworkConfigRestorePlan,
    pub restored_ids: Vec<String>,
    pub pending: Vec<String>,
}

fn server_matches_connection(profile: &ServerProfile, dial: &SshDialTarget) -> bool {
    profile.host.eq_ignore_ascii_case(&dial.host)
        && profile.port == dial.port
        && profile.username == dial.username
}

fn check_remote_config_host(
    host: &dyn Host,
    instance: &AppInstance,
    servers: Option<&[ServerProfile]>,
) -> Result<(), String> {
    let Some(server_id) = server_id_of_host(&instance.host_id) else {
        return Ok(());
    };
    let Some(servers) = servers else {
        return Ok(());
    };
    let profile = servers
        .iter()
        .find(|server| server.id == server_id)
        .ok_or("目标服务器档案不存在，请配置远端后重试")?;
    let dial = host
        .ssh_dial_target()
        .ok_or("无法核对 SSH 连接目标，请在远端页重新测试连接后重试")?;
    if !server_matches_connection(profile, &dial) {
        return Err("当前 SSH 连接与服务器档案不一致，请在远端页重新测试连接后重试".into());
    }
    Ok(())
}

#[cfg(all(test, windows))]
#[path = "config_backup_tests.rs"]
mod tests;

impl AppManager {
    pub async fn framework_config_transfer_guard(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.framework_config_gate.lock().await
    }

    pub async fn framework_config_recoveries(
        &self,
    ) -> Result<Vec<FrameworkConfigRecovery>, String> {
        let directory = self.data_root.join("config/framework-configs");
        if !directory.exists() {
            return Ok(Vec::new());
        }
        let canonical_root = self.data_root.canonicalize().map_err(|e| e.to_string())?;
        let canonical_dir = directory.canonicalize().map_err(|e| e.to_string())?;
        if !canonical_dir.starts_with(&canonical_root)
            || directory
                .symlink_metadata()
                .map_err(|e| e.to_string())?
                .file_type()
                .is_symlink()
        {
            return Err("框架配置恢复目录不能是链接或位于数据目录之外".into());
        }
        let mut entries = tokio::fs::read_dir(&directory)
            .await
            .map_err(|e| e.to_string())?;
        let mut recoveries = Vec::new();
        let mut total = 0usize;
        while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(id) = name.strip_suffix(".json") else {
                continue;
            };
            if !safe_instance_id(id) {
                return Err(format!("非法框架配置恢复文件名: {name}"));
            }
            if !entry
                .file_type()
                .await
                .map_err(|e| e.to_string())?
                .is_file()
            {
                return Err(format!("框架配置恢复文件不是普通文件: {name}"));
            }
            if entry.metadata().await.map_err(|e| e.to_string())?.len() > 16 * 1024 * 1024 {
                return Err(format!("框架配置恢复文件过大: {name}"));
            }
            let bytes = tokio::fs::read(entry.path())
                .await
                .map_err(|e| e.to_string())?;
            total += bytes.len();
            if bytes.len() > 16 * 1024 * 1024
                || total > 64 * 1024 * 1024
                || recoveries.len() >= 1024
            {
                return Err("框架配置恢复文件总量超限".into());
            }
            let recovery: FrameworkConfigRecovery =
                serde_json::from_slice(&bytes).map_err(|e| format!("{name} 无法读取: {e}"))?;
            recovery
                .backup
                .validate()
                .map_err(|e| format!("{name}: {e}"))?;
            if recovery.backup.instance_id != id {
                return Err(format!("框架配置恢复文件与实例 ID 不符: {name}"));
            }
            recoveries.push(recovery);
        }
        recoveries.sort_by(|a, b| a.backup.instance_id.cmp(&b.backup.instance_id));
        Ok(recoveries)
    }

    /// 调用方持有 transfer_guard；远端读不到配置时报错，不能把不完整导出当成成功。
    pub async fn export_framework_config_backups(
        &self,
        instances: &[AppInstance],
        servers: Option<&[ServerProfile]>,
    ) -> Result<Vec<FrameworkConfigBackup>, String> {
        let recoveries = self.framework_config_recoveries().await?;
        let mut backups = Vec::new();
        let mut total = 0usize;
        for instance in instances {
            if let Some(recovery) = recoveries
                .iter()
                .find(|r| r.pending && r.backup.instance_id == instance.id.as_str())
            {
                if recovery.backup.framework_id != instance.framework_id.as_str() {
                    return Err(format!("{} 的待恢复配置框架不匹配", instance.display_name));
                }
                total += serde_json::to_vec(&recovery.backup)
                    .map_err(|e| e.to_string())?
                    .len();
                if total > 64 * 1024 * 1024 {
                    return Err("框架配置备份总量超过 64 MiB".into());
                }
                backups.push(recovery.backup.clone());
                continue;
            }
            if !instance.state.is_installed() {
                continue;
            }
            let backup = async {
                let adapter = self
                    .registry
                    .get(&instance.framework_id)
                    .map_err(|e| e.to_string())?;
                let host = self
                    .resolve_host(&instance.host_id)
                    .await
                    .map_err(|e| e.to_string())?;
                check_remote_config_host(host.as_ref(), instance, servers)?;
                capture_config_backup(host.as_ref(), adapter.as_ref(), instance).await
            }
            .await
            .map_err(|e| {
                format!(
                    "备份框架 {} ({}) 失败: {e}",
                    instance.display_name, instance.id
                )
            })?;
            total += serde_json::to_vec(&backup)
                .map_err(|e| e.to_string())?
                .len();
            if total > 64 * 1024 * 1024 {
                return Err("框架配置备份总量超过 64 MiB".into());
            }
            backups.push(backup);
        }
        Ok(backups)
    }

    /// 所有目标都在 JSON/cache 提交前解析，避免持有实例缓存写锁时重新读实例导致死锁。
    pub async fn prepare_framework_config_restore(
        &self,
        backups: &[FrameworkConfigBackup],
        targets: &[AppInstance],
        servers: Option<&[ServerProfile]>,
    ) -> Result<PreparedFrameworkRestore, String> {
        let current = self.store.list().await;
        let mut result = PreparedFrameworkRestore {
            plan: FrameworkConfigRestorePlan::default(),
            restored_ids: Vec::new(),
            pending: Vec::new(),
        };
        for backup in backups {
            backup.validate()?;
            let Some(instance) = targets.iter().find(|i| i.id.as_str() == backup.instance_id)
            else {
                result
                    .pending
                    .push(format!("{}：目标实例不存在", backup.instance_id));
                continue;
            };
            if instance.framework_id.as_str() != backup.framework_id {
                return Err(format!("{} 的框架配置与目标不匹配", instance.display_name));
            }
            let pending_reason = if matches!(instance.state, AppInstanceState::Installing)
                || current
                    .iter()
                    .any(|i| i.id == instance.id && matches!(i.state, AppInstanceState::Installing))
            {
                Some("实例正在安装，请安装完成后重试".to_string())
            } else if current
                .iter()
                .any(|i| i.id == instance.id && matches!(i.state, AppInstanceState::Running))
            {
                Some("实例正在运行，请停止后重试".to_string())
            } else {
                None
            };
            if let Some(reason) = pending_reason {
                result
                    .pending
                    .push(format!("{}：{reason}", instance.display_name));
                continue;
            }
            let adapter = self
                .registry
                .get(&instance.framework_id)
                .map_err(|e| e.to_string())?;
            let host = match self.resolve_host(&instance.host_id).await {
                Ok(host) => host,
                Err(error) => {
                    result.pending.push(format!(
                        "{}：主机暂不可用，请配置连接后重试（{error}）",
                        instance.display_name
                    ));
                    continue;
                }
            };
            let root = HostPath::from_posix(&instance.install_dir);
            if let Err(reason) = check_remote_config_host(host.as_ref(), instance, servers) {
                result
                    .pending
                    .push(format!("{}：{reason}", instance.display_name));
                continue;
            }
            if !checked_install_root(host.as_ref(), &root).await? {
                result.pending.push(format!(
                    "{}：安装目录不存在，请安装框架或调整实例路径后重试",
                    instance.display_name
                ));
                continue;
            }
            let probe = match adapter.probe_project(host.as_ref(), &root).await {
                Ok(probe) => probe,
                Err(error) => {
                    result.pending.push(format!(
                        "{}：目录未识别为可用框架，请完成安装后重试（{error}）",
                        instance.display_name
                    ));
                    continue;
                }
            };
            let running = match self
                .runtime
                .is_running_readonly(host.as_ref(), instance)
                .await
            {
                Ok(running) => running,
                Err(error) => {
                    result.pending.push(format!(
                        "{}：无法确认实例已停止，请检查主机后重试（{error}）",
                        instance.display_name
                    ));
                    continue;
                }
            };
            if probe.framework_id != instance.framework_id || probe.running || running {
                result.pending.push(format!(
                    "{}：{}",
                    instance.display_name,
                    if probe.running || running {
                        "实例正在运行，请停止后重试"
                    } else {
                        "框架目录不匹配，请检查实例路径"
                    }
                ));
                continue;
            }
            result.plan.add(host, instance, backup).await?;
            result.restored_ids.push(backup.instance_id.clone());
        }
        Ok(result)
    }

    pub fn framework_configs_restored(&self, ids: &[String]) {
        for id in ids {
            self.forget_webui_endpoint(&AppInstanceId::from(id.as_str()));
        }
    }

    pub async fn pending_framework_config_restores(&self) -> Result<Vec<String>, String> {
        let instances = self.store.list().await;
        Ok(self
            .framework_config_recoveries()
            .await?
            .into_iter()
            .filter(|r| r.pending)
            .map(|r| {
                instances
                    .iter()
                    .find(|i| i.id.as_str() == r.backup.instance_id)
                    .map(|i| i.display_name.clone())
                    .unwrap_or(r.backup.instance_id)
            })
            .collect())
    }
}
