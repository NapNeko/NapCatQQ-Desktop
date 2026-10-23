//! Bot 迁移时应用实例的目标构造、依赖重建与元数据交接。

use super::*;
use crate::app_framework::adopt::AdoptSnapshot;
use ncd_domain::DeploymentTaskStatus;

impl AppManager {
    pub(super) async fn migration_app_target(
        &self,
        original: &AppInstance,
        target_host_id: &str,
        job_id: &str,
    ) -> Result<AppInstance, String> {
        if original.host_id == target_host_id {
            return Err("应用端的源主机和目标主机相同".into());
        }
        migration_path_segment(original.id.as_str())?;
        migration_path_segment(job_id)?;
        let lock_name = migration_lock_name(original.framework_id.as_str())?;
        let source_host = self
            .resolve_host(&original.host_id)
            .await
            .map_err(|e| e.to_string())?;
        let source_lock = HostPath::from_posix(&original.install_dir).join(lock_name);
        if !source_host
            .exists(&source_lock)
            .await
            .map_err(|e| e.to_string())?
        {
            return Err(format!(
                "{} 缺少 {lock_name}，无法按原版本重建依赖",
                original.display_name
            ));
        }
        let host = self
            .resolve_host(target_host_id)
            .await
            .map_err(|e| e.to_string())?;
        if host.os() != Os::Linux || host.locality() != Locality::Remote {
            return Err("应用端迁移目前只支持远端 Linux 主机".into());
        }
        let root = self
            .preview_install_dir(target_host_id, &original.framework_id)
            .await
            .map_err(|e| e.to_string())?;
        let requested = root.join(format!("{}-migration-{job_id}", original.id.as_str()));
        let install_dir = self
            .resolve_install_dir(
                host.as_ref(),
                target_host_id,
                &original.framework_id,
                &original.id,
                Some(requested.as_posix()),
            )
            .await
            .map_err(|e| e.to_string())?;
        let mut target = original.clone();
        target.host_id = target_host_id.to_string();
        target.placement = AppPlacement::RemoteNative;
        target.install_dir = install_dir.as_posix().to_string();
        target.state = AppInstanceState::Stopped;
        target.last_error = None;
        target.auto_start = false;
        // 自定义项目的启动入口必须保留；目标另存不含源 systemd 单元的领养快照。
        target.origin = AppInstanceOrigin::Imported;
        if let Some(link) = target.link.as_mut() {
            link.resident_forward_port = None;
        }
        Ok(target)
    }

    pub(super) async fn migration_stop_app(&self, original: &AppInstance) -> Result<(), String> {
        let host = self
            .resolve_host(&original.host_id)
            .await
            .map_err(|e| e.to_string())?;
        self.runtime
            .stop(host, original)
            .await
            .map_err(|e| e.to_string())?;
        self.drop_instance_tunnel(&original.id).await;
        if let Some(link) = original.link.as_ref()
            && link.resident_forward_port.is_some()
        {
            self.teardown_resident_best_effort(original, &link.bot_id)
                .await;
        }
        let saved = self
            .store
            .update(&original.id, |instance| {
                instance.state = AppInstanceState::Stopped;
                instance.auto_start = false;
                instance.last_error = None;
            })
            .await
            .map_err(|e| e.to_string())?;
        self.forget_webui_endpoint(&original.id);
        self.publish(&saved, "migration_stopped");
        Ok(())
    }

    pub(super) async fn migration_rebuild_app(&self, target: &AppInstance) -> Result<(), String> {
        let lock_name = migration_lock_name(target.framework_id.as_str())?;
        let host = self
            .resolve_host(&target.host_id)
            .await
            .map_err(|e| e.to_string())?;
        if host.os() != Os::Linux || host.locality() != Locality::Remote {
            return Err("应用端迁移目前只支持远端 Linux 主机".into());
        }
        let root = HostPath::from_posix(&target.install_dir);
        let lock_path = root.join(lock_name);
        if !host.exists(&lock_path).await.map_err(|e| e.to_string())? {
            return Err(format!(
                "{} 缺少 {lock_name}，无法按原版本重建依赖",
                target.display_name
            ));
        }
        let lock_before = host
            .read_file(&lock_path)
            .await
            .map_err(|e| e.to_string())?;
        let adapter = self
            .registry
            .get(&target.framework_id)
            .map_err(|e| e.to_string())?;
        let component_id = ComponentId::parse(&adapter.manifest().component_id)
            .ok_or_else(|| format!("未注册的应用端组件：{}", target.framework_id))?;
        let components = self.component_executor().map_err(|e| e.to_string())?;
        let mut inputs = components
            .inputs_for(&target.host_id, host.as_ref(), None)
            .await;
        let mut hint = self.component_hint(target, target.installed_version.clone());
        hint.adopt_existing = true;
        inputs.app_component = Some(hint);
        let task_id = components
            .submit(ComponentActionRequest {
                component_id,
                host_id: target.host_id.clone(),
                kind: StepKind::ForceInstall,
                task_id: None,
                host: Arc::clone(&host),
                inputs,
            })
            .await?;
        // 不能走 track_install：实例表此时仍指向源主机。
        loop {
            match components.deployment_tasks().status_of(&task_id).await {
                Some((DeploymentTaskStatus::Success, _)) => break,
                Some((status, error)) if status.is_terminal() => {
                    return Err(error.unwrap_or_else(|| {
                        format!("{} 的目标依赖重建未成功结束", target.display_name)
                    }));
                }
                Some(_) => tokio::time::sleep(INSTALL_POLL_INTERVAL).await,
                None => return Err("应用端依赖重建任务已丢失，请保留目标目录后重试".into()),
            }
        }
        let lock_after = host
            .read_file(&lock_path)
            .await
            .map_err(|e| e.to_string())?;
        if lock_after != lock_before {
            return Err(format!(
                "{} 重建依赖时改动了 {lock_name}，迁移已停止",
                target.display_name
            ));
        }
        let probe = adapter
            .probe_project(host.as_ref(), &root)
            .await
            .map_err(|e| e.to_string())?;
        if !probe.ready {
            return Err(format!("{} 的目标依赖尚未就绪", target.display_name));
        }
        Ok(())
    }

    pub(super) async fn migration_adopt_snapshot(
        &self,
        id: &AppInstanceId,
    ) -> Result<Option<AdoptSnapshot>, String> {
        self.adopt_store.load(id).await.map_err(|e| e.to_string())
    }

    pub(super) async fn migration_save_adopt_target(
        &self,
        target: &AppInstance,
    ) -> Result<(), String> {
        let host = self
            .resolve_host(&target.host_id)
            .await
            .map_err(|e| e.to_string())?;
        let adapter = self
            .registry
            .get(&target.framework_id)
            .map_err(|e| e.to_string())?;
        let root = HostPath::from_posix(&target.install_dir);
        let rels = adapter
            .adopt_watch_rels(host.as_ref(), &root)
            .await
            .map_err(|e| e.to_string())?;
        let snapshot = adopt::capture_snapshot(
            host.as_ref(),
            &target.id,
            &target.host_id,
            &target.install_dir,
            &rels,
            &[],
            now_ms(),
        )
        .await
        .map_err(|e| e.to_string())?;
        self.adopt_store
            .save(&snapshot)
            .await
            .map_err(|e| e.to_string())
    }

    pub(super) async fn migration_restore_adopt(
        &self,
        source: &AppInstance,
        snapshot: Option<&AdoptSnapshot>,
    ) -> Result<(), String> {
        match snapshot {
            Some(snapshot) => {
                if snapshot.instance_id != source.id.as_str()
                    || snapshot.host_id != source.host_id
                    || snapshot.install_dir != source.install_dir
                {
                    return Err("领养快照与源应用端实例不一致".into());
                }
                self.adopt_store
                    .save(snapshot)
                    .await
                    .map_err(|e| e.to_string())
            }
            None => self
                .adopt_store
                .remove(&source.id)
                .await
                .map_err(|e| e.to_string()),
        }
    }

    pub(super) async fn migration_set_app(&self, instance: AppInstance) -> Result<(), String> {
        let moved = match self.store.get(&instance.id).await {
            Some(current) => {
                current.host_id != instance.host_id || current.install_dir != instance.install_dir
            }
            None => true,
        };
        if moved {
            self.drop_instance_tunnel(&instance.id).await;
        }
        self.forget_webui_endpoint(&instance.id);
        if let Ok(adapter) = self.registry.get(&instance.framework_id) {
            adapter.forget_panel_session(instance.id.as_str());
        }
        self.config_write_slots
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&instance.id);
        let saved = self
            .store
            .upsert(instance)
            .await
            .map_err(|e| e.to_string())?;
        self.publish(&saved, "migrated");
        Ok(())
    }

    pub(super) async fn migration_relink_app(
        &self,
        id: &AppInstanceId,
        bot_id: &BotId,
    ) -> Result<(), String> {
        let instance = self.store.require(id).await.map_err(|e| e.to_string())?;
        if let Some(link) = instance.link.as_ref()
            && link.bot_id != *bot_id
        {
            return Err("应用端仍对接着另一个 Bot，无法自动迁移对接".into());
        }
        let plan = self
            .preview_link(id, bot_id)
            .await
            .map_err(|e| e.to_string())?;
        if let Some(link) = instance.link.as_ref()
            && link.mode != plan.mode
        {
            return Err("目标对接方向与原实例不一致，迁移已停止".into());
        }
        self.apply_link_inner(id, bot_id)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn migration_lock_name(framework: &str) -> Result<&'static str, String> {
    match framework {
        "nonebot2" => Ok("uv.lock"),
        "karin" => Ok("pnpm-lock.yaml"),
        "yunzai" => Err("云崽迁移还需要迁移 Redis 数据，当前暂不支持自动迁移".into()),
        _ => Err(format!("{framework} 尚不支持保留原项目的依赖重建")),
    }
}

fn migration_path_segment(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
    {
        return Err("迁移任务或应用实例标识不合法".into());
    }
    Ok(())
}
