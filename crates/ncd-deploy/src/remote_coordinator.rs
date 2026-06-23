use std::collections::HashMap;
use std::sync::Arc;

use ncd_host::{Host, HostPath};

/// per-server mutex 协调器，防止同服务器多 bot 并发翻转 package.json 导致竞态。
///
/// 只序列化 flip + 验证的临界区；进程启动、daemon 引导、隧道等长耗时操作
/// 仍在锁外并发执行。
pub struct RemoteQqEntryCoordinator {
    per_server: Arc<tokio::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>,
}

impl Default for RemoteQqEntryCoordinator {
    fn default() -> Self {
        Self {
            per_server: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }
}

impl RemoteQqEntryCoordinator {
    /// 以 per-server 锁保护 f 执行。
    pub async fn with_server<F, Fut, R>(&self, server_id: &str, f: F) -> R
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = R>,
    {
        let inner = {
            let mut map = self.per_server.lock().await;
            map.entry(server_id.to_string())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        let _permit = inner.lock().await;
        f().await
    }

    /// 确保远端共享 QQ 树处于 NapCat 注入模式（loadNapCat.js 存在）。
    ///
    /// 远端原生 NC bot 的唯一翻转入口。
    pub async fn ensure_for_napcat(
        &self,
        host: &dyn Host,
        server_id: &str,
        install_base: &HostPath,
    ) -> Result<(), String> {
        use ncd_component::remote_qq_entry::{QQ_MAIN_NAPCAT_INJECT, set_remote_qq_package_main};

        self.with_server(server_id, || async {
            let pkg_path = install_base.join("opt/QQ/resources/app/package.json");
            let desired = QQ_MAIN_NAPCAT_INJECT;

            let mut current = None;
            if let Ok(bytes) = host.read_file(&pkg_path).await {
                if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                    current = v.get("main").and_then(|m| m.as_str()).map(str::to_string);
                }
            }

            if current.as_deref() != Some(desired) {
                set_remote_qq_package_main(host, install_base, desired)
                    .await
                    .map_err(|e| format!("patch package.json main to napcat-inject failed: {e}"))?;
            }

            let load_js = install_base.join("opt/QQ/resources/app/loadNapCat.js");
            if !host.exists(&load_js).await.map_err(|e| e.to_string())? {
                return Err(format!(
                    "远端未找到 NapCat 注入入口脚本 {}（server_id={}）。\
                     请先到「组件」页为该 SSH 主机安装 NapCat 组件（该步骤会写入 loadNapCat.js 并修改 package.json）。",
                    load_js.as_posix(),
                    server_id
                ));
            }

            let napcat_mjs = install_base.join("opt/QQ/resources/app/app_launcher/napcat/napcat.mjs");
            if !host.exists(&napcat_mjs).await.map_err(|e| e.to_string())? {
                return Err(format!(
                    "远端未找到 NapCat 核心模块 {}。请先在组件页安装 NapCat。",
                    napcat_mjs.as_posix()
                ));
            }

            Ok(())
        })
        .await
    }

    /// 确保远端共享 QQ 树处于 vanilla/native 模式（SnowLuma 冷启路径）。
    ///
    /// SL 冷启先起裸 QQ 进程，再由远端 daemon ptrace 注入。
    pub async fn ensure_for_native(
        &self,
        host: &dyn Host,
        server_id: &str,
        install_base: &HostPath,
    ) -> Result<(), String> {
        use ncd_component::remote_qq_entry::{QQ_MAIN_NATIVE, set_remote_qq_package_main};

        self.with_server(server_id, || async {
            let pkg_path = install_base.join("opt/QQ/resources/app/package.json");
            let desired = QQ_MAIN_NATIVE;

            let mut current = None;
            if let Ok(bytes) = host.read_file(&pkg_path).await {
                if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                    current = v.get("main").and_then(|m| m.as_str()).map(str::to_string);
                }
            }

            if current.as_deref() != Some(desired) {
                set_remote_qq_package_main(host, install_base, desired)
                    .await
                    .map_err(|e| format!("patch package.json main to native failed: {e}"))?;
            }

            Ok(())
        })
        .await
    }
}
