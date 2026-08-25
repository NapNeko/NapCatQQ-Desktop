//! 远端 SnowLuma 路径布局与可执行文件探测
//!
//! 与 src-tauri/commands/components.rs 远端 Linux 分支一致:
//! workspace = $HOME/snowluma-remote/workspace,framework 在 workspace/snowluma/;
//! QQ 与 NapCat 组件页相同,装在 $HOME/Napcat/opt/QQ/qq

use ncd_domain::RemoteSelectedPaths;
use ncd_domain::{
    derive_remote_linux_paths, join_under, qq_bin_candidates, qq_install_base_from_qq_bin,
    require_qq_install_base, require_snowluma_dir, snowluma_workspace_from_dir,
};
use ncd_host::{Host, HostCommand, HostPath};

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

/// NapCat/QQ 组件页 rootless 安装路径（桌面默认树，不是启动回落）
pub fn napcat_layout_qq_executable(home: &str) -> String {
    qq_bin_candidates(home)
        .into_iter()
        .next()
        .unwrap_or_else(|| ncd_domain::qq_bin(&join_under(home, "Napcat")))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteSnowLumaLayout {
    pub home: String,
    pub paths: SnowLumaRemotePaths,
    pub node_bin: String,
    pub qq_bin: String,
    /// 与 `qq_bin` 同一棵树的安装根（`/opt/QQ/qq` → `/`）
    pub qq_install_base: String,
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
    let script = format!("test -f '{escaped}' && test -x '{escaped}'");
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
    let bundled_nested = format!("{}/node/bin/node", paths.snowluma_dir);
    for candidate in [paths.bundled_node_bin(), bundled_nested, paths.node_bin()] {
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

pub use ncd_domain::snowluma_install_candidates;

/// 一次探测 home + 路径 + node/qq 可执行文件
pub async fn probe_remote_snowluma_layout(
    host: &dyn Host,
) -> Result<RemoteSnowLumaLayout, BotBackendError> {
    let home = probe_remote_home(host).await?;
    let selected = RemoteSelectedPaths {
        home,
        ..RemoteSelectedPaths::default()
    };
    layout_from_selected_or_probe(host, &selected).await
}

/// 库存有 snowluma_dir 就用；没有则按固定候选目录探测（进程停了库存会丢路径）。
pub async fn layout_from_selected_or_probe(
    host: &dyn Host,
    selected: &RemoteSelectedPaths,
) -> Result<RemoteSnowLumaLayout, BotBackendError> {
    let mut selected = selected.clone();
    if selected
        .snowluma_dir
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_none()
    {
        let home = if selected.home.trim().is_empty() {
            probe_remote_home(host).await?
        } else {
            selected.home.clone()
        };
        selected.home = home.clone();
        let found = find_snowluma_install(host, &home).await.ok_or_else(|| {
            BotBackendError::InvalidConfig(format!(
                "远端未发现 SnowLuma framework（home={home}）。请在组件页安装或填写覆盖路径后重新发现。"
            ))
        })?;
        selected.snowluma_workspace = Some(snowluma_workspace_from_dir(&found));
        selected.snowluma_dir = Some(found);
    }
    if selected
        .qq_bin
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_none()
    {
        for qq in qq_bin_candidates(&selected.home) {
            if host_path_executable(host, &qq).await {
                if selected
                    .qq_install_base
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .is_none()
                {
                    selected.qq_install_base = qq_install_base_from_qq_bin(&qq);
                }
                selected.qq_bin = Some(qq);
                break;
            }
        }
    }
    let mut layout = layout_from_selected(&selected)?;
    if node_bin_needs_reprobe(selected.node_bin.as_deref(), &layout.paths.snowluma_dir) {
        layout.node_bin = resolve_node_bin(host, &layout.paths).await?;
    }
    Ok(layout)
}

/// 库存里的 `/usr/bin/node` 不能直接拿来启动：完整包自带 Node 22，系统 Node 往往过旧。
pub(crate) fn node_bin_needs_reprobe(selected_node: Option<&str>, snowluma_dir: &str) -> bool {
    match selected_node.map(str::trim).filter(|s| !s.is_empty()) {
        Some(bin)
            if ncd_domain::is_bundled_snowluma_node(bin, Some(snowluma_dir))
                || ncd_domain::is_portable_lite_node(bin, Some(snowluma_dir)) =>
        {
            false
        }
        _ => true,
    }
}

async fn find_snowluma_install(host: &dyn Host, home: &str) -> Option<String> {
    for dir in snowluma_install_candidates(home) {
        if snowluma_dir_is_install(host, &dir).await {
            return Some(dir);
        }
    }
    None
}

async fn snowluma_dir_is_install(host: &dyn Host, dir: &str) -> bool {
    let entry = HostPath::from_posix(format!("{dir}/index.mjs"));
    if !host.exists(&entry).await.unwrap_or(false) {
        return false;
    }
    let webui = HostPath::from_posix(format!("{dir}/config/webui.json"));
    if host.exists(&webui).await.unwrap_or(false) {
        return true;
    }
    let runtime = HostPath::from_posix(format!("{dir}/config/runtime.json"));
    let Ok(bytes) = host.read_file(&runtime).await else {
        return false;
    };
    String::from_utf8_lossy(&bytes).contains("webuiPort")
}

/// 用库存选中路径构造布局；缺项带上实际 home / source 线索
pub fn layout_from_selected(
    selected: &RemoteSelectedPaths,
) -> Result<RemoteSnowLumaLayout, BotBackendError> {
    let derived = derive_remote_linux_paths(selected);
    let qq_install_base =
        require_qq_install_base(selected).map_err(BotBackendError::InvalidConfig)?;
    let snowluma_dir = require_snowluma_dir(selected).map_err(BotBackendError::InvalidConfig)?;
    let workspace = derived
        .snowluma_workspace
        .clone()
        .unwrap_or_else(|| snowluma_workspace_from_dir(&snowluma_dir));
    let qq_bin = derived.qq_bin.clone().ok_or_else(|| {
        BotBackendError::InvalidConfig(format!(
            "远端未发现 QQ（home={}）。请在组件页安装 QQ 或填写覆盖路径后重新发现。",
            selected.home
        ))
    })?;
    let node_bin = derived
        .node_bin
        .clone()
        .unwrap_or_else(|| join_under(&snowluma_dir, "node"));
    Ok(RemoteSnowLumaLayout {
        home: derived.home,
        paths: SnowLumaRemotePaths::from_workspace_and_dir(&workspace, &snowluma_dir),
        node_bin,
        qq_bin,
        qq_install_base,
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
    fn install_candidates_include_opt_snowluma() {
        let c = snowluma_install_candidates("/root");
        assert_eq!(c[0], "/root/snowluma-remote/workspace/snowluma");
        assert_eq!(c[1], "/root/Napcat/snowluma-workspace/snowluma");
        assert_eq!(c[2], "/opt/snowluma");
    }

    #[test]
    fn system_path_node_must_be_reprobed_for_bundled_runtime() {
        assert!(node_bin_needs_reprobe(
            Some("/usr/bin/node"),
            "/opt/snowluma"
        ));
        assert!(node_bin_needs_reprobe(None, "/opt/snowluma"));
        assert!(!node_bin_needs_reprobe(
            Some("/opt/snowluma/node"),
            "/opt/snowluma"
        ));
        assert!(!node_bin_needs_reprobe(
            Some("/opt/snowluma/node/bin/node"),
            "/opt/snowluma"
        ));
        assert!(!node_bin_needs_reprobe(
            Some("/home/u/snowluma-remote/workspace/node/bin/node"),
            "/home/u/snowluma-remote/workspace/snowluma"
        ));
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
        assert_eq!(layout.qq_install_base, "/data/qq");
        assert_eq!(layout.paths.snowluma_dir, "/data/sl");
        assert_eq!(layout.node_bin, "/usr/bin/node");
        assert_eq!(layout.paths.bundled_node_bin(), "/data/sl/node");
    }

    #[test]
    fn layout_from_selected_system_qq_does_not_use_home_napcat() {
        let selected = ncd_domain::RemoteSelectedPaths {
            home: "/root".into(),
            qq_bin: Some("/opt/QQ/qq".into()),
            snowluma_dir: Some("/opt/snowluma".into()),
            snowluma_workspace: Some("/opt/snowluma".into()),
            node_bin: Some("/opt/snowluma/node".into()),
            ..ncd_domain::RemoteSelectedPaths::default()
        };
        let layout = layout_from_selected(&selected).unwrap();
        assert_eq!(layout.qq_install_base, "/");
        assert_eq!(layout.qq_bin, "/opt/QQ/qq");
        assert_ne!(layout.qq_install_base, "/root/Napcat");
    }
}
