//! 远端 daemon 前置条件、配置渲染与场景判定

use std::collections::HashMap;

use ncd_deploy::backend_config_renderer::render_snowluma_docker_config_payloads;
use ncd_domain::{BackendType, BotConfig, BotId, RuntimeScenario, SnowLumaStartMode};
use ncd_host::{Host, HostCommand, HostPath};
use ncd_traits::runtime_backend::BotBackendError;
use serde_json::{Value, json};

use super::layout::{DEFAULT_WEBUI_PORT, SnowLumaRemotePaths, napcat_layout_qq_executable};
use super::orchestrator::resolve_remote_bash;
use crate::snowluma::session::{build_webui_json_payload, generate_strong_password};

use super::helpers::host_file_nonempty;

pub(crate) async fn ensure_remote_daemon_prereqs(
    host: &dyn Host,
    home: &str,
    paths: &SnowLumaRemotePaths,
) -> Result<(), BotBackendError> {
    host.create_dir_all(&HostPath::from_posix(&paths.config_dir))
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;

    let runtime_json = serde_json::to_vec_pretty(&json!({ "webuiPort": DEFAULT_WEBUI_PORT }))
        .map_err(|e| BotBackendError::Json(e.to_string()))?;
    host.write_file(
        &HostPath::from_posix(format!("{}/runtime.json", paths.config_dir)),
        &runtime_json,
    )
    .await
    .map_err(|e| BotBackendError::Io(e.to_string()))?;

    let webui_plain = if host_file_nonempty(host, &paths.webui_secret).await {
        let bytes = host
            .read_file(&HostPath::from_posix(&paths.webui_secret))
            .await
            .map_err(|e| BotBackendError::Io(e.to_string()))?;
        String::from_utf8_lossy(&bytes).trim().to_string()
    } else {
        let pwd = generate_strong_password(16);
        host.write_file(&HostPath::from_posix(&paths.webui_secret), pwd.as_bytes())
            .await
            .map_err(|e| BotBackendError::Io(e.to_string()))?;
        pwd
    };

    if webui_plain.is_empty() {
        return Err(BotBackendError::InvalidConfig(
            "远端 webui.secret 为空，无法启动 SnowLuma daemon".into(),
        ));
    }

    let webui_payload = build_webui_json_payload(&webui_plain, false)
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    let webui_json = serde_json::to_vec_pretty(&webui_payload)
        .map_err(|e| BotBackendError::Json(e.to_string()))?;
    host.write_file(
        &HostPath::from_posix(format!("{}/webui.json", paths.config_dir)),
        &webui_json,
    )
    .await
    .map_err(|e| BotBackendError::Io(e.to_string()))?;

    if !host_file_nonempty(host, &paths.vnc_secret).await {
        let vnc_pwd = generate_strong_password(8);
        host.write_file(&HostPath::from_posix(&paths.vnc_secret), vnc_pwd.as_bytes())
            .await
            .map_err(|e| BotBackendError::Io(e.to_string()))?;
    }

    let stack_check = HostCommand::new("sh").arg("-c").arg(
        "command -v Xvfb >/dev/null && command -v x11vnc >/dev/null && \
         command -v websockify >/dev/null && command -v dbus-launch >/dev/null",
    );
    let stack = host
        .run_to_string(stack_check)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    if !stack.success() {
        return Err(BotBackendError::InvalidConfig(
            "远端缺少 SnowLuma 图形栈（需要 Xvfb、x11vnc、websockify、dbus-launch）。\
             请先在远端安装依赖（或参考 legacy install_snowluma 脚本）。"
                .into(),
        ));
    }

    resolve_remote_bash(host).await?;

    let qq = napcat_layout_qq_executable(home);
    let qq_check = HostCommand::new("sh")
        .arg("-c")
        .arg(format!("test -x '{}'", qq.replace('\'', "'\"'\"'")));
    let qq_out = host
        .run_to_string(qq_check)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    if !qq_out.success() {
        return Err(BotBackendError::InvalidConfig(format!(
            "远端未找到可执行的 QQ（组件页应已安装到 {qq}）。请先在同一 SSH 主机安装 QQ 组件。"
        )));
    }

    Ok(())
}

pub async fn render_native_snowluma_config_on_host(
    host: &dyn Host,
    bot_id: &BotId,
    config: &BotConfig,
    paths: &SnowLumaRemotePaths,
) -> Result<(), BotBackendError> {
    if config.bot.backend_type != BackendType::SnowLuma {
        return Err(BotBackendError::InvalidConfig(
            "render_native_snowluma_config_on_host 仅支持 SnowLuma".into(),
        ));
    }
    host.create_dir_all(&HostPath::from_posix(&paths.config_dir))
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    let config_dir = &paths.config_dir;
    let mut existing = HashMap::new();
    let file_name = format!("onebot_{}.json", bot_id.as_str());
    let path = HostPath::from_posix(format!("{config_dir}/{file_name}"));
    if let Ok(bytes) = host.read_file(&path).await {
        if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
            existing.insert(file_name.clone(), value);
        }
    }
    for item in render_snowluma_docker_config_payloads(bot_id, config, &existing) {
        let bytes = serde_json::to_vec_pretty(&item.payload)
            .map_err(|e| BotBackendError::Json(e.to_string()))?;
        let p = HostPath::from_posix(format!("{config_dir}/{}", item.file_name));
        host.write_file(&p, &bytes)
            .await
            .map_err(|e| BotBackendError::Io(e.to_string()))?;
    }
    Ok(())
}

pub(crate) fn resolve_start_mode(config: &BotConfig) -> SnowLumaStartMode {
    config
        .bot
        .snowluma_start_mode
        .unwrap_or(SnowLumaStartMode::ColdStart)
}

/// 远端 Native + SnowLuma + SSH 主机
pub fn is_remote_native_snowluma_config(config: &BotConfig) -> bool {
    RuntimeScenario::from_config(config)
        .map(|scenario| scenario.is_remote_native_snowluma())
        .unwrap_or(false)
}
