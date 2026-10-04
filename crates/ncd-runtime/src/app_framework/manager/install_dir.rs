//! 实例装在哪：默认目录、用户自选目录，以及按实例目录探测组件

use super::*;

/// 本机实例目录：`data_root/apps/<framework>/<instance>`
const LOCAL_APPS_DIR: &str = "apps";

/// 远端实例目录：`$HOME/ncd/apps/<framework>/<instance>`
const REMOTE_APPS_REL: &str = "ncd/apps";

pub fn parse_user_install_dir(raw: &str, os: Os) -> Result<HostPath, AppFrameworkError> {
    let t = raw.trim();
    if t.is_empty() {
        return Err(AppFrameworkError::Validation("安装目录为空".into()));
    }
    let path = match os {
        Os::Windows => {
            if t.starts_with('/') {
                HostPath::from_posix(t)
            } else {
                HostPath::from_windows(t)
            }
        }
        _ => HostPath::from_posix(t),
    };
    if !path.is_absolute() {
        return Err(AppFrameworkError::Validation(
            "安装目录必须是绝对路径".into(),
        ));
    }
    Ok(path)
}

impl AppManager {
    pub(super) fn component_spec(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> AppComponentSpec {
        AppComponentSpec {
            install_dir: HostPath::from_posix(&instance.install_dir),
            port: instance.port,
            node_bin: self.managed_component_dir(host, "NodeJs").map(|dir| {
                ncd_component::NodeJsComponent::node_binary_path_for_os(&dir, host.os())
            }),
            uv_bin: self
                .managed_component_dir(host, "Uv")
                .map(|dir| ncd_component::UvComponent::uv_binary_path_for_os(&dir, host.os())),
            git_bin: self.managed_component_dir(host, "Git").and_then(|dir| {
                ncd_component::GitComponent::managed_binary_path_for_os(&dir, host.os())
            }),
            redis_bin: self.managed_component_dir(host, "Redis").map(|dir| {
                ncd_component::RedisComponent::managed_binary_path_for_os(&dir, host.os())
            }),
            npm_registry: self.npm_registry.clone(),
            // 版本 / PyPI 镜像由安装请求给（AppComponentHint），这里只用于探测与起停
            pypi_index: None,
            install_version: None,
            install_renderer: instance.install_renderer,
            adopt_existing: instance.origin.is_imported(),
            instance_id: instance.id.as_str().to_string(),
            webui_username: None,
            webui_password: None,
        }
    }

    /// 桌面端管理的运行时依赖落点（与对应组件的安装目录一致）；远端安装时由 factory 按远端路径
    /// 推断并写进实例标记，起停 / 探测时框架组件读标记，这里只管本机
    fn managed_component_dir(&self, host: &dyn Host, name: &str) -> Option<HostPath> {
        if host.locality() != Locality::Local {
            return None;
        }
        Some(
            data_root_to_host_path(&self.data_root, host.os())
                .join("components")
                .join(name),
        )
    }

    pub(super) async fn detect(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<DetectOutcome, AppFrameworkError> {
        let adapter = self.registry.get(&instance.framework_id)?;
        let component = adapter.component(&self.component_spec(host, instance));
        component
            .detect_outcome(host)
            .await
            .map_err(|e| AppFrameworkError::Runtime(e.to_string()))
    }

    pub async fn preview_install_dir(
        &self,
        host_id: &str,
        framework_id: &AppFrameworkId,
    ) -> Result<HostPath, AppFrameworkError> {
        let host = self.resolve_host(host_id).await?;
        self.apps_root_for(host.as_ref(), framework_id).await
    }

    pub async fn resolve_install_dir(
        &self,
        host: &dyn Host,
        host_id: &str,
        framework: &AppFrameworkId,
        id: &AppInstanceId,
        override_dir: Option<&str>,
    ) -> Result<HostPath, AppFrameworkError> {
        let path = match override_dir.map(str::trim).filter(|s| !s.is_empty()) {
            None => self.install_dir_for(host, framework, id).await?,
            Some(raw) => parse_user_install_dir(raw, host.os())?,
        };
        let posix = path.as_posix();
        let taken = self
            .store
            .list()
            .await
            .into_iter()
            .any(|i| i.host_id == host_id && i.id != *id && i.install_dir == posix);
        if taken {
            return Err(AppFrameworkError::Validation(
                "该目录已被其它实例占用".to_string(),
            ));
        }
        let exists = host
            .exists(&path)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
        if exists {
            match host.list_dir(&path).await {
                Ok(entries) if entries.is_empty() => {}
                Ok(_) => {
                    return Err(AppFrameworkError::Validation(
                        "目录非空，请选空文件夹或不存在的路径".to_string(),
                    ));
                }
                Err(_) => {
                    return Err(AppFrameworkError::Validation(
                        "安装路径必须是目录".to_string(),
                    ));
                }
            }
        }
        Ok(path)
    }

    pub(super) async fn bind_existing_dir(
        &self,
        host: &dyn Host,
        host_id: &str,
        id: &AppInstanceId,
        override_dir: Option<&str>,
    ) -> Result<HostPath, AppFrameworkError> {
        let raw = override_dir
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AppFrameworkError::Validation("导入必须指定已有项目目录".into()))?;
        let path = parse_user_install_dir(raw, host.os())?;
        let posix = path.as_posix();
        let taken = self
            .store
            .list()
            .await
            .into_iter()
            .any(|i| i.host_id == host_id && i.id != *id && i.install_dir == posix);
        if taken {
            return Err(AppFrameworkError::Validation(
                "该目录已被其它实例占用".to_string(),
            ));
        }
        if !host
            .exists(&path)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?
        {
            return Err(AppFrameworkError::Validation("目录不存在".into()));
        }
        if host.list_dir(&path).await.is_err() {
            return Err(AppFrameworkError::Validation("导入路径必须是目录".into()));
        }
        Ok(path)
    }

    async fn apps_root_for(
        &self,
        host: &dyn Host,
        framework: &AppFrameworkId,
    ) -> Result<HostPath, AppFrameworkError> {
        match host.locality() {
            Locality::Local => Ok(data_root_to_host_path(&self.data_root, host.os())
                .join(LOCAL_APPS_DIR)
                .join(framework.as_str())),
            Locality::Remote => {
                let out = host
                    .run_to_string(
                        HostCommand::new("sh")
                            .arg("-c")
                            .arg("printf '%s' \"$HOME\"")
                            .timeout(Duration::from_secs(15)),
                    )
                    .await
                    .map_err(host_err)?;
                let home = out.stdout.trim();
                if home.is_empty() || !home.starts_with('/') {
                    return Err(AppFrameworkError::Host(
                        "无法解析远端 $HOME，不能决定安装目录".to_string(),
                    ));
                }
                Ok(HostPath::from_posix(home)
                    .join(REMOTE_APPS_REL)
                    .join(framework.as_str()))
            }
        }
    }

    async fn install_dir_for(
        &self,
        host: &dyn Host,
        framework: &AppFrameworkId,
        id: &AppInstanceId,
    ) -> Result<HostPath, AppFrameworkError> {
        Ok(self.apps_root_for(host, framework).await?.join(id.as_str()))
    }

    #[cfg(test)]
    pub(super) async fn resolve_install_dir_for_test(
        &self,
        host_id: &str,
        framework: &AppFrameworkId,
        id: &AppInstanceId,
        override_dir: Option<&str>,
    ) -> Result<HostPath, AppFrameworkError> {
        let host = self.resolve_host(host_id).await?;
        self.resolve_install_dir(host.as_ref(), host_id, framework, id, override_dir)
            .await
    }
}
