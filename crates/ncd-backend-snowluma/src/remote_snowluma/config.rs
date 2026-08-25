//! 远端 daemon 前置条件、配置渲染与场景判定

use std::collections::HashMap;

use ncd_deploy::backend_config_renderer::render_snowluma_docker_config_payloads;
use ncd_domain::{BackendType, BotConfig, BotId, RuntimeScenario, SnowLumaStartMode};
use ncd_host::{Host, HostCommand, HostPath};
use ncd_traits::runtime_backend::BotBackendError;
use serde_json::{Value, json};

use super::layout::{DEFAULT_WEBUI_PORT, SnowLumaRemotePaths, napcat_layout_qq_executable};
use super::orchestrator::resolve_remote_bash;
use crate::snowluma::session::{
    build_webui_json_payload, generate_strong_password, verify_webui_password,
};

use super::helpers::host_file_nonempty;

pub(crate) async fn ensure_remote_daemon_prereqs(
    host: &dyn Host,
    home: &str,
    paths: &SnowLumaRemotePaths,
    qq_bin: &str,
) -> Result<(), BotBackendError> {
    host.create_dir_all(&HostPath::from_posix(&paths.config_dir))
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;

    let runtime_path = format!("{}/runtime.json", paths.config_dir);
    if !host_file_nonempty(host, &runtime_path).await {
        let runtime_json = serde_json::to_vec_pretty(&json!({ "webuiPort": DEFAULT_WEBUI_PORT }))
            .map_err(|e| BotBackendError::Json(e.to_string()))?;
        host.write_file(&HostPath::from_posix(&runtime_path), &runtime_json)
            .await
            .map_err(|e| BotBackendError::Io(e.to_string()))?;
    }

    sync_remote_webui_credentials(host, paths).await?;

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

    let qq = if qq_bin.trim().is_empty() {
        napcat_layout_qq_executable(home)
    } else {
        qq_bin.to_string()
    };
    let qq_check = HostCommand::new("sh")
        .arg("-c")
        .arg(format!("test -x '{}'", qq.replace('\'', "'\"'\"'")));
    let qq_out = host
        .run_to_string(qq_check)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    if !qq_out.success() {
        return Err(BotBackendError::InvalidConfig(format!(
            "远端未找到可执行的 QQ（期望 {qq}）。请先在同一 SSH 主机安装 QQ 组件。"
        )));
    }

    Ok(())
}

/// 已有 webui.json 时不改写哈希（导入的现成安装）。明文只在 secret 能对上哈希时使用。
async fn sync_remote_webui_credentials(
    host: &dyn Host,
    paths: &SnowLumaRemotePaths,
) -> Result<(), BotBackendError> {
    let webui_json_path = format!("{}/webui.json", paths.config_dir);
    let existing_hash = read_webui_hash_salt(host, &webui_json_path).await;
    let secret = if host_file_nonempty(host, &paths.webui_secret).await {
        let bytes = host
            .read_file(&HostPath::from_posix(&paths.webui_secret))
            .await
            .map_err(|e| BotBackendError::Io(e.to_string()))?;
        String::from_utf8_lossy(&bytes).trim().to_string()
    } else {
        String::new()
    };

    match existing_hash {
        Some((hash, salt)) => {
            if !secret.is_empty() && verify_webui_password(&secret, &hash, &salt) {
                return Ok(());
            }
            Ok(())
        }
        None => {
            let plain = if secret.is_empty() {
                let pwd = generate_strong_password(16);
                host.write_file(&HostPath::from_posix(&paths.webui_secret), pwd.as_bytes())
                    .await
                    .map_err(|e| BotBackendError::Io(e.to_string()))?;
                pwd
            } else {
                secret
            };
            let webui_payload = build_webui_json_payload(&plain, false)
                .map_err(|e| BotBackendError::Io(e.to_string()))?;
            let webui_json = serde_json::to_vec_pretty(&webui_payload)
                .map_err(|e| BotBackendError::Json(e.to_string()))?;
            host.write_file(&HostPath::from_posix(&webui_json_path), &webui_json)
                .await
                .map_err(|e| BotBackendError::Io(e.to_string()))?;
            Ok(())
        }
    }
}

async fn read_webui_hash_salt(host: &dyn Host, path: &str) -> Option<(String, String)> {
    let bytes = host.read_file(&HostPath::from_posix(path)).await.ok()?;
    let v: Value = serde_json::from_slice(&bytes).ok()?;
    let hash = v.get("passwordHash")?.as_str()?.trim().to_string();
    let salt = v.get("passwordSalt")?.as_str()?.trim().to_string();
    if hash.is_empty() || salt.is_empty() {
        return None;
    }
    Some((hash, salt))
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
