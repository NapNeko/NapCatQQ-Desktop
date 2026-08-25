//! 远端 SnowLuma 路径布局与可执行文件探测
//!
//! 与 src-tauri/commands/components.rs 远端 Linux 分支一致:
//! workspace = $HOME/snowluma-remote/workspace,framework 在 workspace/snowluma/;
//! QQ 与 NapCat 组件页相同,装在 $HOME/Napcat/opt/QQ/qq

use ncd_domain::RemoteSelectedPaths;
use ncd_host::{Host, HostCommand};

use ncd_traits::runtime_backend::BotBackendError;

pub const DEFAULT_DISPLAY_NUM: i32 = 0;
pub const DEFAULT_VNC_PORT: i32 = 5900;
pub const DEFAULT_NOVNC_PORT: i32 = 6081;
pub const DEFAULT_WEBUI_PORT: i32 = 5099;

/// 远端 SnowLuma 目录布局(对齐 legacy SnowLumaRemotePaths.from_base)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnowLumaRemotePaths {
    pub workspace_dir: String,
    pub snowluma_dir: String,
    pub config_dir: String,
    pub runtime_dir: String,
    pub log_dir: String,
    pub vnc_secret: String,
    pub webui_secret: String,
    pub pid_daemon: String,
    pub status_daemon: String,
    pub log_daemon: String,
    pub dbus_env: String,
}

impl SnowLumaRemotePaths {
    pub fn from_remote_home(home: &str) -> Self {
        let workspace = format!("{home}/snowluma-remote/workspace");
        let snowluma = format!("{workspace}/snowluma");
        Self::from_workspace_and_dir(&workspace, &snowluma)
    }

    pub fn from_workspace_and_dir(workspace: &str, snowluma: &str) -> Self {
        Self {
            workspace_dir: workspace.to_string(),
            snowluma_dir: snowluma.to_string(),
            config_dir: format!("{snowluma}/config"),
            runtime_dir: format!("{workspace}/runtime"),
            log_dir: format!("{workspace}/log"),
            vnc_secret: format!("{workspace}/vnc.secret"),
            webui_secret: format!("{workspace}/webui.secret"),
            pid_daemon: format!("{workspace}/runtime/pid_daemon"),
            status_daemon: format!("{workspace}/runtime/status_daemon.json"),
            log_daemon: format!("{workspace}/log/daemon.log"),
            dbus_env: format!("{workspace}/runtime/dbus.env"),
        }
    }

    pub fn node_bin(&self) -> String {
        format!("{}/node/bin/node", self.workspace_dir)
    }

    /// 官方完整包把 node 放在 framework 根下（与 launcher.sh 一致）
    pub fn bundled_node_bin(&self) -> String {
        format!("{}/node", self.snowluma_dir)
    }

    pub fn status_bot_path(&self, qq_id: &str) -> String {
        format!("{}/status_bot_{qq_id}.json", self.runtime_dir)
    }

    pub fn pid_bot_path(&self, qq_id: &str) -> String {
        format!("{}/pid_bot_{qq_id}", self.runtime_dir)
    }

    pub fn pid_node_path(&self) -> String {
        format!("{}/pid_node", self.runtime_dir)
    }

    pub fn log_bot_path(&self, qq_id: &str) -> String {
        format!("{}/bot_{qq_id}.log", self.log_dir)
    }
}

/// NapCat/QQ 组件页 rootless 安装路径
pub fn napcat_layout_qq_executable(home: &str) -> String {
    format!("{home}/Napcat/opt/QQ/qq")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteSnowLumaLayout {
    pub home: String,
    pub paths: SnowLumaRemotePaths,
    pub node_bin: String,
    pub qq_bin: String,
}

pub async fn probe_remote_home(host: &dyn Host) -> Result<String, BotBackendError> {
    let cmd = HostCommand::new("sh").arg("-c").arg("echo \"$HOME\"");
    let out = host
        .run_to_string(cmd)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    if !out.success() {
        return Err(BotBackendError::Io("探测远端 $HOME 失败".into()));
    }
    let home = out.stdout.lines().next().unwrap_or("").trim().to_string();
    if home.is_empty() {
        return Err(BotBackendError::InvalidConfig(
            "远端 $HOME 为空，无法派生 SnowLuma 路径".into(),
        ));
    }
    Ok(home)
}

async fn host_path_executable(host: &dyn Host, path: &str) -> bool {
    let escaped = path.replace('\'', "'\"'\"'");
    let script = format!("test -x '{escaped}'");
    let cmd = HostCommand::new("sh").arg("-c").arg(script);
    host.run_to_string(cmd)
        .await
        .ok()
        .is_some_and(|o| o.success())
}

pub async fn resolve_node_bin(
    host: &dyn Host,
    paths: &SnowLumaRemotePaths,
) -> Result<String, BotBackendError> {
    for candidate in [paths.bundled_node_bin(), paths.node_bin()] {
        if host_path_executable(host, &candidate).await {
            return Ok(candidate);
        }
    }
    let cmd = HostCommand::new("sh").arg("-c").arg("command -v node");
    let out = host
        .run_to_string(cmd)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    if out.success() {
        let line = out.stdout.lines().next().unwrap_or("").trim();
        if !line.is_empty() {
            return Ok(line.to_string());
        }
    }
    Err(BotBackendError::InvalidConfig(
        "远端未找到 node（官方完整包应自带 ./node；旧 lite 请安装 Node.js 组件或提供 PATH 中的 Node 22.13+）。"
            .into(),
    ))
}

/// 一次探测 home + 路径 + node/qq 可执行文件
pub async fn probe_remote_snowluma_layout(
    host: &dyn Host,
) -> Result<RemoteSnowLumaLayout, BotBackendError> {
    let home = probe_remote_home(host).await?;
    let paths = SnowLumaRemotePaths::from_remote_home(&home);
    let node_bin = resolve_node_bin(host, &paths).await?;
    let qq_bin = napcat_layout_qq_executable(&home);
    if !host_path_executable(host, &qq_bin).await {
        return Err(BotBackendError::InvalidConfig(format!(
            "远端未找到可执行的 QQ（组件页应已安装到 {qq_bin}）。请先在同一 SSH 主机安装 QQ 组件。"
        )));
    }
    let entry = format!("{}/index.mjs", paths.snowluma_dir);
    let check = HostCommand::new("sh")
        .arg("-c")
        .arg(format!("test -f '{}'", entry.replace('\'', "'\"'\"'")));
    let ok = host
        .run_to_string(check)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?
        .success();
    if !ok {
        return Err(BotBackendError::InvalidConfig(format!(
            "远端 SnowLuma framework 未安装（缺少 {entry}）。请先在组件页安装 SnowLuma。"
        )));
    }
    Ok(RemoteSnowLumaLayout {
        home,
        paths,
        node_bin,
        qq_bin,
    })
}

/// 用库存选中路径构造布局；缺项带上实际 home / source 线索
pub fn layout_from_selected(
    selected: &RemoteSelectedPaths,
) -> Result<RemoteSnowLumaLayout, BotBackendError> {
    let home = selected.home.clone();
    let snowluma_dir = selected.snowluma_dir.clone().ok_or_else(|| {
        BotBackendError::InvalidConfig(format!(
            "远端未发现 SnowLuma framework（home={home}）。请在组件页安装或填写覆盖路径后重新发现。"
        ))
    })?;
    let workspace = selected
        .snowluma_workspace
        .clone()
        .unwrap_or_else(|| snowluma_dir.trim_end_matches("/snowluma").to_string());
    let qq_bin = selected.qq_bin.clone().ok_or_else(|| {
        BotBackendError::InvalidConfig(format!(
            "远端未发现 QQ（home={home}）。请在组件页安装 QQ 或填写覆盖路径后重新发现。"
        ))
    })?;
    let node_bin = selected
        .snowluma_dir
        .as_ref()
        .map(|d| format!("{d}/node"))
        .or_else(|| selected.node_bin.clone())
        .ok_or_else(|| {
            BotBackendError::InvalidConfig(format!(
                "远端未找到 node（home={home}）。官方完整包应自带 ./node；旧 lite 请安装 Node.js 组件。"
            ))
        })?;
    Ok(RemoteSnowLumaLayout {
        home,
        paths: SnowLumaRemotePaths::from_workspace_and_dir(&workspace, &snowluma_dir),
        node_bin,
        qq_bin,
    })
}

pub fn shell_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_match_components_page_workspace() {
        let p = SnowLumaRemotePaths::from_remote_home("/home/u");
        assert_eq!(p.workspace_dir, "/home/u/snowluma-remote/workspace");
        assert_eq!(p.snowluma_dir, "/home/u/snowluma-remote/workspace/snowluma");
        assert_eq!(
            p.node_bin(),
            "/home/u/snowluma-remote/workspace/node/bin/node"
        );
        assert_eq!(
            p.pid_node_path(),
            "/home/u/snowluma-remote/workspace/runtime/pid_node"
        );
    }

    #[test]
    fn qq_bin_uses_napcat_install_base() {
        assert_eq!(
            napcat_layout_qq_executable("/home/u"),
            "/home/u/Napcat/opt/QQ/qq"
        );
    }

    #[test]
    fn layout_from_selected_uses_custom_paths() {
        let selected = ncd_domain::RemoteSelectedPaths {
            home: "/home/u".into(),
            qq_install_base: Some("/data/qq".into()),
            qq_bin: Some("/data/qq/opt/QQ/qq".into()),
            snowluma_dir: Some("/data/sl".into()),
            snowluma_workspace: Some("/data/sl".into()),
            node_bin: Some("/usr/bin/node".into()),
            napcat_root: None,
            ncd_watch_root: None,
            needs_sudo: false,
        };
        let layout = layout_from_selected(&selected).unwrap();
        assert_eq!(layout.qq_bin, "/data/qq/opt/QQ/qq");
        assert_eq!(layout.paths.snowluma_dir, "/data/sl");
        assert_eq!(layout.node_bin, "/data/sl/node");
        assert_eq!(layout.paths.bundled_node_bin(), "/data/sl/node");
    }
}
