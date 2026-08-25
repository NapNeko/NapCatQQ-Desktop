//! 远端 Linux「直接运行」启动规划(无 napcat.sh 脚本)
//!
//! 对齐组件页 RemoteLayout + NapCatComponent::launch_command,在 SSH Host 上
//! spawn 进程;启动前把 onebot/napcat 配置写到远端
//! $HOME/Napcat/opt/QQ/.../napcat/config/(或 system 布局 /opt/QQ/...)

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use ncd_component::{Component, LaunchArgs, NapCatComponent, linux_qq_running_pid_script};
use ncd_deploy::{DeploymentError, NativeLaunchCommand, NativeLaunchTranslator};
use ncd_domain::{BackendType, BotConfig, BotFlavor, BotId, RemoteSelectedPaths};
use ncd_host::{Host, HostCommand, HostPath};

use ncd_deploy::backend_config_renderer::render_napcat_docker_config_payloads;
use ncd_traits::runtime_backend::BotBackendError;

/// 与 src-tauri/commands/components.rs::RemoteLayout 同语义
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteNapcatLayout {
    System,
    Rootless,
}

/// 探测远端 $HOME 与 NapCat 安装布局(一次 shell 往返)
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

#[cfg(test)]
mod selected_tests {
    use super::*;
    use ncd_domain::RemoteSelectedPaths;

    #[test]
    fn napcat_paths_from_selected_custom_prefix() {
        let selected = RemoteSelectedPaths {
            home: "/home/u".into(),
            qq_install_base: Some("/data/qq".into()),
            qq_bin: Some("/data/qq/opt/QQ/qq".into()),
            needs_sudo: false,
            ..RemoteSelectedPaths::default()
        };
        let (home, layout, base) = napcat_paths_from_selected(&selected).unwrap();
        assert_eq!(home, "/home/u");
        assert_eq!(layout, RemoteNapcatLayout::Rootless);
        assert_eq!(base.as_posix(), "/data/qq");
    }

    #[test]
    fn napcat_paths_from_selected_missing_qq_errors() {
        let selected = RemoteSelectedPaths {
            home: "/home/u".into(),
            ..RemoteSelectedPaths::default()
        };
        let err = napcat_paths_from_selected(&selected).unwrap_err();
        assert!(err.contains("/home/u"));
        assert!(err.contains("未发现"));
    }
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

/// 导入迁移：读取 `{napcat_root}/config/onebot11_{qq}.json`。
/// 文件不存在返回 Ok(None)；存在但缺少 network 返回 Err。
pub async fn read_remote_napcat_connect(
    host: &dyn Host,
    napcat_root: &str,
    qq_id: &str,
) -> Result<Option<ncd_domain::ImportedNetworkConfig>, String> {
    let path = format!(
        "{}/config/onebot11_{}.json",
        napcat_root.trim_end_matches('/'),
        qq_id
    );
    let bytes = match host.read_file(&HostPath::from_posix(&path)).await {
        Ok(bytes) => bytes,
        Err(ncd_host::HostError::PathNotFound { .. }) => {
            tracing::info!(
                target: "ncd_backend_napcat::remote",
                %path,
                "远端 NapCat onebot11 配置不存在，跳过网络配置迁移"
            );
            return Ok(None);
        }
        Err(err) => {
            return Err(format!("读取 {path} 失败: {err}"));
        }
    };
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("解析 {path} 失败: {e}"))?;
    ncd_deploy::backend_config_renderer::parse_napcat_onebot_connect(&value)
        .map(Some)
        .ok_or_else(|| format!("{path} 缺少 network 字段，无法迁移网络配置"))
}

pub fn napcat_remote_log_path(install_base: &HostPath, qq_id: u64) -> String {
    format!("{}/log/napcat_{qq_id}.log", install_base.as_posix())
}

/// 把 NapCat 派生配置写到远端 config 目录(与 Python write_bot_runtime_config 同路径语义)
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

/// xvfb-run -a <qq> --no-sandbox -q <qq_id>,与 legacy launcher 核心一致(无 bash 脚本)
///
/// `metrics_env`：可选 NCD_METRICS_*，export 进 nohup 子 shell 作兜底。
/// 探针主路径是 loadNapCat.js 内 bake env + require；不要带 NODE_OPTIONS=--require
///（会在入口脚本写 env 前预加载并被 module cache 锁成 disabled）。
async fn build_napcat_remote_launch(
    host: &dyn Host,
    config: &BotConfig,
    install_base: &HostPath,
    metrics_env: &std::collections::BTreeMap<String, String>,
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

    // export NCD_METRICS_* 兜底；主路径是 loadNapCat 内 bake（勿 export NODE_OPTIONS=--require 探针）
    let mut env_exports = String::new();
    for (k, v) in metrics_env {
        if k.is_empty() || k == "NODE_OPTIONS" {
            continue;
        }
        env_exports.push_str(&format!("export {}={}; ", k, shell_single_quote(v)));
    }
    // 合并 qq_cmd.environment
    for (k, v) in &qq_cmd.environment {
        if metrics_env.contains_key(k) {
            continue;
        }
        env_exports.push_str(&format!("export {}={}; ", k, shell_single_quote(v)));
    }

    let inner = format!(
        "{env_exports}nohup xvfb-run -a {qq_invoke} >> {log_q} 2>&1 </dev/null & wait $! || true"
    );

    Ok(NativeLaunchCommand {
        program: "sh".into(),
        args: vec!["-c".into(), inner],
        working_dir: qq_cmd.working_dir.map(|p| PathBuf::from(p.as_posix())),
        // env 已 bake 进 shell；仍透传一份便于日志/调试
        environment: {
            let mut e = qq_cmd.environment.clone();
            e.extend(metrics_env.iter().map(|(k, v)| (k.clone(), v.clone())));
            e
        },
    })
}

fn shell_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}

/// 远端指标注入上下文（由 ncd-runtime 在 wiring 时注入）
#[async_trait]
pub trait RemoteMetricsInjector: Send + Sync {
    /// 返回应 export 进 QQ 启动 shell 的 env；None = 指标关或失败（不阻断启动）
    async fn prepare_napcat(
        &self,
        host: &dyn Host,
        home: &str,
        bot_id: &str,
        config: &BotConfig,
        install_base: &HostPath,
    ) -> Option<std::collections::BTreeMap<String, String>>;
}

/// 按 runtime_target 在远端 Host 上翻译 Native 启动命令
pub struct RemoteNativeLaunchTranslator {
    host: Arc<dyn Host>,
    flavor: BotFlavor,
    /// server_id of the remote (used for per-host entry point coordination).
    server_id: String,
    /// Shared coordinator so that concurrent batch starts (or mixed NC+SL on the same host)
    /// serialize the flip of the shared package.json main + artifact verification.
    coordinator: Arc<ncd_deploy::remote_coordinator::RemoteQqEntryCoordinator>,
    /// 可选：远端实例指标探针注入（失败不阻断启动）
    metrics_injector: Option<Arc<dyn RemoteMetricsInjector>>,
    cached_layout: tokio::sync::Mutex<Option<(String, RemoteNapcatLayout, HostPath)>>,
    selected: Option<RemoteSelectedPaths>,
}

impl RemoteNativeLaunchTranslator {
    /// 供 ncd-runtime facade 层统一构造远端启动 translator。
    pub fn new(
        host: Arc<dyn Host>,
        flavor: BotFlavor,
        server_id: String,
        coordinator: Arc<ncd_deploy::remote_coordinator::RemoteQqEntryCoordinator>,
    ) -> Self {
        Self::new_with_metrics(host, flavor, server_id, coordinator, None)
    }

    pub fn new_with_metrics(
        host: Arc<dyn Host>,
        flavor: BotFlavor,
        server_id: String,
        coordinator: Arc<ncd_deploy::remote_coordinator::RemoteQqEntryCoordinator>,
        metrics_injector: Option<Arc<dyn RemoteMetricsInjector>>,
    ) -> Self {
        Self {
            host,
            flavor,
            server_id,
            coordinator,
            metrics_injector,
            cached_layout: tokio::sync::Mutex::new(None),
            selected: None,
        }
    }

    pub fn with_selected(mut self, selected: RemoteSelectedPaths) -> Self {
        self.selected = Some(selected);
        self
    }

    async fn layout(&self) -> Result<(String, RemoteNapcatLayout, HostPath), DeploymentError> {
        let mut guard = self.cached_layout.lock().await;
        if let Some(triple) = guard.as_ref() {
            return Ok(triple.clone());
        }
        let triple = if let Some(sel) = &self.selected {
            napcat_paths_from_selected(sel).map_err(DeploymentError::LaunchFailed)?
        } else {
            let (home, layout) = probe_remote_napcat_layout(self.host.as_ref())
                .await
                .map_err(DeploymentError::LaunchFailed)?;
            let install_base =
                napcat_install_base(&home, layout).map_err(DeploymentError::LaunchFailed)?;
            (home, layout, install_base)
        };
        *guard = Some(triple.clone());
        Ok(triple)
    }
}

pub fn napcat_paths_from_selected(
    selected: &RemoteSelectedPaths,
) -> Result<(String, RemoteNapcatLayout, HostPath), String> {
    let home = selected.home.clone();
    let base = selected.qq_install_base.clone().ok_or_else(|| {
        format!(
            "远端未发现 QQ 安装树（home={}）。请在组件页安装或填写覆盖路径后重新发现。",
            home
        )
    })?;
    let layout = if selected.needs_sudo {
        RemoteNapcatLayout::System
    } else {
        RemoteNapcatLayout::Rootless
    };
    Ok((home, layout, HostPath::from_posix(base)))
}

#[async_trait]
impl NativeLaunchTranslator for RemoteNativeLaunchTranslator {
    async fn translate(&self, config: &BotConfig) -> Result<NativeLaunchCommand, DeploymentError> {
        match self.flavor {
            BotFlavor::NapCat => {
                let bot_id = BotId::new(config.bot.qq_id.to_string());
                let (home, _layout, install_base) = self.layout().await?;

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

                let mut metrics_env = std::collections::BTreeMap::new();
                if let Some(inj) = &self.metrics_injector {
                    if let Some(env) = inj
                        .prepare_napcat(
                            self.host.as_ref(),
                            &home,
                            bot_id.as_str(),
                            config,
                            &install_base,
                        )
                        .await
                    {
                        metrics_env = env;
                    }
                }

                build_napcat_remote_launch(
                    self.host.as_ref(),
                    config,
                    &install_base,
                    &metrics_env,
                )
                .await
            }
            BotFlavor::SnowLuma => Err(DeploymentError::LaunchFailed(
                "远端 SnowLuma 走 RemoteSnowLumaBackend + RemoteSnowLumaDaemon（非 NativeDeployment 单进程模型）。"
                    .into(),
            )),
        }
    }
}

/// 停止远端 NapCat QQ 进程(pgrep + SIGTERM/SIGKILL,对齐 legacy launcher stop 语义)
///
/// 匹配策略(从严到宽,始终要求命令行里出现 `qq` + `-q <qq_id>` 结尾):
/// 1. 精确: `qq --no-sandbox -q <qq>`
/// 2. 回退: `qq` 与 `-q <qq>` 同 cmdline(允许路径/参数顺序变化)
///
/// 不用裸 `-q <qq>`,避免误杀其它工具。
pub async fn stop_remote_napcat_on_host(
    host: &dyn Host,
    qq_id: u64,
) -> Result<(), BotBackendError> {
    let script = format!(
        r#"qq_id="{qq_id}"
pids="$(pgrep -f -- "qq --no-sandbox -q ${{qq_id}}$" 2>/dev/null || true)"
if [ -z "$pids" ]; then
  pids="$(pgrep -f -- "qq.*-q ${{qq_id}}$" 2>/dev/null || true)"
fi
if [ -z "$pids" ]; then exit 0; fi
echo "$pids" | while read -r pid; do
  [ -z "$pid" ] && continue
  kill "$pid" 2>/dev/null || true
done
sleep 1
pids="$(pgrep -f -- "qq --no-sandbox -q ${{qq_id}}$" 2>/dev/null || true)"
if [ -z "$pids" ]; then
  pids="$(pgrep -f -- "qq.*-q ${{qq_id}}$" 2>/dev/null || true)"
fi
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

/// 探测远端 NapCat/QQ 是否在跑（cmdline `-q` 或 Ptlogin2 当前账号，不写死 WebUI 口）
pub async fn remote_napcat_running_pid(
    host: &dyn Host,
    qq_id: u64,
) -> Result<Option<u32>, BotBackendError> {
    let script = linux_qq_running_pid_script(qq_id, None, None);
    let cmd = HostCommand::new("sh").arg("-c").arg(script);
    let out = host
        .run_to_string(cmd)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    if !out.success() {
        if out.stdout.trim().is_empty() {
            return Ok(None);
        }
        return Err(BotBackendError::Io(format!(
            "远端 QQ 进程探测失败: exit={:?} stderr={}",
            out.exit_code,
            out.stderr.trim()
        )));
    }
    let line = out.stdout.lines().next().unwrap_or("").trim();
    if line.is_empty() {
        return Ok(None);
    }
    line.parse()
        .map(Some)
        .map_err(|_| BotBackendError::InvalidConfig(format!("invalid qq pid: {line}")))
}
