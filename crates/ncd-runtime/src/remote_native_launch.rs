//! 远端 Linux「直接运行」启动规划（无 napcat.sh 脚本）。
//!
//! 对齐组件页 `RemoteLayout` + `NapCatComponent::launch_command`，在 SSH Host 上
//! `spawn` 进程；启动前把 onebot/napcat 配置写到远端
//! `$HOME/Napcat/opt/QQ/.../napcat/config/`（或 system 布局 `/opt/QQ/...`）。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use ncd_component::{Component, LaunchArgs, NapCatComponent};
use ncd_deploy::{DeploymentError, NativeLaunchCommand, NativeLaunchTranslator};
use ncd_domain::{BackendType, BotConfig, BotFlavor, BotId};
use ncd_host::{Host, HostCommand, HostPath};

use crate::backend_config_renderer::render_napcat_docker_config_payloads;
use crate::runtime_backend::BotBackendError;

/// 与 `src-tauri/commands/components.rs::RemoteLayout` 同语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteNapcatLayout {
    System,
    Rootless,
}

/// 探测远端 $HOME 与 NapCat 安装布局（一次 shell 往返）。
pub async fn probe_remote_napcat_layout(
    host: &dyn Host,
) -> Result<(String, RemoteNapcatLayout), String> {
    let script = "echo \"$HOME\"; \
         test -e /opt/QQ/resources/app/app_launcher/napcat/napcat.mjs && echo 1 || echo 0";
    let cmd = HostCommand::new("sh").arg("-c").arg(script);
    let out = host
        .run_to_string(cmd)
        .await
        .map_err(|e| format!("探测远端布局失败: {e}"))?;
    if !out.success() {
        return Err(format!(
            "探测远端布局失败: exit={:?} stderr={}",
            out.exit_code,
            out.stderr.trim()
        ));
    }
    let mut lines = out.stdout.lines();
    let home = lines
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "无法探测远端 $HOME，请确认 SSH 用户家目录可用。".to_string())?
        .to_string();
    let system_exists = lines.next().map(str::trim) == Some("1");
    let layout = if system_exists {
        RemoteNapcatLayout::System
    } else {
        RemoteNapcatLayout::Rootless
    };
    Ok((home, layout))
}

fn napcat_install_base(home: &str, layout: RemoteNapcatLayout) -> Result<HostPath, String> {
    Ok(match layout {
        RemoteNapcatLayout::System => HostPath::from_posix("/"),
        RemoteNapcatLayout::Rootless => HostPath::from_posix(format!("{home}/Napcat")),
    })
}

fn napcat_config_dir(install_base: &HostPath) -> String {
    format!(
        "{}/opt/QQ/resources/app/app_launcher/napcat/config",
        install_base.as_posix()
    )
}

pub fn napcat_remote_log_path(install_base: &HostPath, qq_id: u64) -> String {
    format!("{}/log/napcat_{qq_id}.log", install_base.as_posix())
}

/// 把 NapCat 派生配置写到远端 config 目录（与 Python `write_bot_runtime_config` 同路径语义）。
pub async fn render_native_napcat_config_on_host(
    host: &dyn Host,
    bot_id: &BotId,
    config: &BotConfig,
    install_base: &HostPath,
) -> Result<(), BotBackendError> {
    if config.bot.backend_type != BackendType::NapCat {
        return Err(BotBackendError::InvalidConfig(
            "render_native_napcat_config_on_host 仅支持 NapCat".into(),
        ));
    }
    let config_dir = napcat_config_dir(install_base);
    let config_dir_path = HostPath::from_posix(&config_dir);
    host.create_dir_all(&config_dir_path)
        .await
        .map_err(|e| BotBackendError::Io(format!("创建远端 NapCat 配置目录失败: {e}")))?;

    let existing = read_existing_napcat_config(host, bot_id, &config_dir).await?;
    for item in render_napcat_docker_config_payloads(bot_id, config, &existing) {
        let bytes = serde_json::to_vec_pretty(&item.payload)
            .map_err(|e| BotBackendError::Json(e.to_string()))?;
        let path = HostPath::from_posix(format!("{config_dir}/{}", item.file_name));
        host.write_file(&path, &bytes)
            .await
            .map_err(|e| BotBackendError::Io(format!("写远端 NapCat 配置失败: {e}")))?;
    }
    Ok(())
}

async fn read_existing_napcat_config(
    host: &dyn Host,
    bot_id: &BotId,
    config_dir: &str,
) -> Result<HashMap<String, serde_json::Value>, BotBackendError> {
    let mut existing = HashMap::new();
    for file_name in [
        format!("onebot11_{}.json", bot_id.as_str()),
        format!("napcat_{}.json", bot_id.as_str()),
    ] {
        let path = HostPath::from_posix(format!("{config_dir}/{file_name}"));
        match host.read_file(&path).await {
            Ok(bytes) => {
                if let Ok(value) = serde_json::from_slice(&bytes) {
                    existing.insert(file_name, value);
                }
            }
            Err(ncd_host::HostError::PathNotFound { .. }) => {}
            Err(error) => return Err(BotBackendError::Io(error.to_string())),
        }
    }
    Ok(existing)
}

/// `xvfb-run -a <qq> --no-sandbox -q <qq_id>`，与 legacy launcher 核心一致（无 bash 脚本）。
async fn build_napcat_remote_launch(
    host: &dyn Host,
    config: &BotConfig,
    install_base: &HostPath,
) -> Result<NativeLaunchCommand, DeploymentError> {
    let qq_id = config.bot.qq_id;
    let component = NapCatComponent::new(install_base.clone());
    let launch_args = LaunchArgs {
        extra_args: vec!["--no-sandbox".into(), "-q".into(), qq_id.to_string()],
        ..Default::default()
    };
    let qq_cmd = component
        .launch_command(host, &launch_args)
        .map_err(|e| DeploymentError::LaunchFailed(e.to_string()))?;

    let check = HostCommand::new("sh")
        .arg("-c")
        .arg("command -v xvfb-run >/dev/null 2>&1");
    let check_out = host
        .run_to_string(check)
        .await
        .map_err(|e| DeploymentError::LaunchFailed(e.to_string()))?;
    if !check_out.success() {
        return Err(DeploymentError::LaunchFailed(
            "远端未安装 xvfb-run，无法无头启动 QQ。请在远端安装 xvfb 或使用 Docker 部署。".into(),
        ));
    }

    let log_path = napcat_remote_log_path(install_base, qq_id);
    let log_parent = log_path.rsplit_once('/').map(|(p, _)| p).unwrap_or(".");
    let rotate = HostCommand::new("sh").arg("-c").arg(format!(
        "mkdir -p {log_parent} && \
         if [ -f {log_path} ]; then mv -f {log_path} {log_path}.prev 2>/dev/null || true; fi && \
         : > {log_path}"
    ));
    host.run_to_string(rotate)
        .await
        .map_err(|e| DeploymentError::LaunchFailed(e.to_string()))?;

    let mut qq_parts = vec![qq_cmd.program.clone()];
    qq_parts.extend(qq_cmd.args.clone());
    let qq_invoke = qq_parts
        .iter()
        .map(|a| shell_single_quote(a))
        .collect::<Vec<_>>()
        .join(" ");
    let log_q = shell_single_quote(&log_path);
    let inner = format!(
        "nohup xvfb-run -a {qq_invoke} >> {log_q} 2>&1 </dev/null & wait $! || true"
    );

    Ok(NativeLaunchCommand {
        program: "sh".into(),
        args: vec!["-c".into(), inner],
        working_dir: qq_cmd
            .working_dir
            .map(|p| PathBuf::from(p.as_posix())),
        environment: qq_cmd.environment,
    })
}

fn shell_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}

/// 按 runtime_target 在远端 Host 上翻译 Native 启动命令。
pub struct RemoteNativeLaunchTranslator {
    host: Arc<dyn Host>,
    flavor: BotFlavor,
    /// server_id of the remote (used for per-host entry point coordination).
    server_id: String,
    /// Shared coordinator so that concurrent batch starts (or mixed NC+SL on the same host)
    /// serialize the flip of the shared `package.json` main + artifact verification.
    coordinator: Arc<crate::bot_manager::RemoteQqEntryCoordinator>,
    cached_layout: tokio::sync::Mutex<Option<(String, RemoteNapcatLayout)>>,
}

impl RemoteNativeLaunchTranslator {
    /// 内部构造器。
    ///
    /// 只应由同 crate 内的 `BotManager` 调用。
    /// 使用 `pub(crate)` 而非 `pub`，以避免把 `pub(crate)` 的 `RemoteQqEntryCoordinator`
    /// 通过公开 API 泄露出去（这正是编译器 private_interfaces 警告的来源）。
    ///
    /// 外部 crate（例如 src-tauri）不应直接构造此类型，所有 backend 都应由
    /// `BotManager::backend_for_config` 统一创建。
    pub(crate) fn new(
        host: Arc<dyn Host>,
        flavor: BotFlavor,
        server_id: String,
        coordinator: Arc<crate::bot_manager::RemoteQqEntryCoordinator>,
    ) -> Self {
        Self {
            host,
            flavor,
            server_id,
            coordinator,
            cached_layout: tokio::sync::Mutex::new(None),
        }
    }

    async fn layout(&self) -> Result<(String, RemoteNapcatLayout), DeploymentError> {
        let mut guard = self.cached_layout.lock().await;
        if let Some(pair) = guard.as_ref() {
            return Ok(pair.clone());
        }
        let pair = probe_remote_napcat_layout(self.host.as_ref())
            .await
            .map_err(DeploymentError::LaunchFailed)?;
        *guard = Some(pair.clone());
        Ok(pair)
    }
}

#[async_trait]
impl NativeLaunchTranslator for RemoteNativeLaunchTranslator {
    async fn translate(&self, config: &BotConfig) -> Result<NativeLaunchCommand, DeploymentError> {
        match self.flavor {
            BotFlavor::NapCat => {
                let bot_id = BotId::new(config.bot.qq_id.to_string());
                let (home, layout) = self.layout().await?;
                let install_base =
                    napcat_install_base(&home, layout).map_err(DeploymentError::LaunchFailed)?;

                // Per-bot config files can be rendered without the entry lock.
                render_native_napcat_config_on_host(
                    self.host.as_ref(),
                    &bot_id,
                    config,
                    &install_base,
                )
                .await
                .map_err(|e| DeploymentError::LaunchFailed(e.to_string()))?;

                // The critical shared operation: switch the common QQ tree to NapCat-injected
                // mode *and* verify that loadNapCat.js + napcat/napcat.mjs actually exist.
                // This is serialized per server_id by the coordinator so batch_start of
                // multiple (possibly mixed NC+SL) bots on the same remote host cannot race
                // the package.json write or launch a QQ that will immediately fail the require.
                self.coordinator
                    .ensure_for_napcat(self.host.as_ref(), &self.server_id, &install_base)
                    .await
                    .map_err(DeploymentError::LaunchFailed)?;

                build_napcat_remote_launch(self.host.as_ref(), config, &install_base).await
            }
            BotFlavor::SnowLuma => Err(DeploymentError::LaunchFailed(
                "远端 SnowLuma 走 RemoteSnowLumaBackend + RemoteSnowLumaDaemon（非 NativeDeployment 单进程模型）。"
                    .into(),
            )),
        }
    }
}

/// 停止远端 NapCat QQ 进程（pgrep + SIGTERM/SIGKILL，对齐 legacy launcher stop 语义）。
pub async fn stop_remote_napcat_on_host(
    host: &dyn Host,
    qq_id: u64,
) -> Result<(), BotBackendError> {
    let script = format!(
        r#"qq_id="{qq_id}"
pids="$(pgrep -f "qq --no-sandbox -q ${{qq_id}}$" 2>/dev/null || true)"
if [ -z "$pids" ]; then exit 0; fi
echo "$pids" | while read -r pid; do
  [ -z "$pid" ] && continue
  kill "$pid" 2>/dev/null || true
done
sleep 1
pids="$(pgrep -f "qq --no-sandbox -q ${{qq_id}}$" 2>/dev/null || true)"
if [ -n "$pids" ]; then
  echo "$pids" | while read -r pid; do
    [ -z "$pid" ] && continue
    kill -9 "$pid" 2>/dev/null || true
  done
fi
"#
    );
    let cmd = HostCommand::new("sh").arg("-c").arg(script);
    let out = host
        .run_to_string(cmd)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    if !out.success() {
        return Err(BotBackendError::Io(format!(
            "远端停止 NapCat 失败: exit={:?} stderr={}",
            out.exit_code,
            out.stderr.trim()
        )));
    }
    Ok(())
}

/// pgrep 探测远端 NapCat 是否在跑。
pub async fn remote_napcat_running_pid(host: &dyn Host, qq_id: u64) -> Result<Option<u32>, BotBackendError> {
    let script = format!(
        r#"pgrep -f "qq --no-sandbox -q {qq_id}$" 2>/dev/null | head -n 1"#
    );
    let cmd = HostCommand::new("sh").arg("-c").arg(script);
    let out = host
        .run_to_string(cmd)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    if !out.success() {
        return Ok(None);
    }
    let line = out.stdout.lines().next().unwrap_or("").trim();
    if line.is_empty() {
        return Ok(None);
    }
    line.parse()
        .map(Some)
        .map_err(|_| BotBackendError::InvalidConfig(format!("invalid pgrep pid: {line}")))
}