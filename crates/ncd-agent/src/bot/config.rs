//! 会话 Bot 的配置写入只使用目标机路径，并保留上游未知字段。
use std::collections::HashMap;
use std::path::{Component, Path};

use ncd_deploy::backend_config_renderer::{
    render_napcat_docker_config_payloads, render_snowluma_docker_config_payloads,
};
use ncd_domain::windows_bot::WindowsBotSpec;
use ncd_domain::{BackendType, BotId};
use serde::Serialize;

pub(super) fn valid_bot_id(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 20
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || value.parse::<u64>().ok().is_none_or(|value| value == 0)
    {
        return Err("Bot 标识必须是非零 QQ 号码".into());
    }
    Ok(())
}

pub(super) fn validate_spec(root: &Path, spec: &WindowsBotSpec) -> Result<(), String> {
    valid_bot_id(&spec.bot_id)?;
    if spec.bot_id != spec.config.bot.qq_id.to_string() {
        return Err("Bot 标识与配置不一致".into());
    }
    let root = root
        .canonicalize()
        .map_err(|error| format!("托管目录不可用：{error}"))?;
    let data = Path::new(&spec.data_root);
    if !data.is_absolute()
        || data
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err("Bot 数据目录必须是目标机绝对路径".into());
    }
    let canonical = data
        .canonicalize()
        .map_err(|error| format!("Bot 数据目录不可用：{error}"))?;
    if !canonical
        .to_string_lossy()
        .to_ascii_lowercase()
        .starts_with(&format!(
            "{}{}",
            root.to_string_lossy()
                .to_ascii_lowercase()
                .trim_end_matches(['\\', '/']),
            std::path::MAIN_SEPARATOR
        ))
        && !canonical
            .to_string_lossy()
            .eq_ignore_ascii_case(&root.to_string_lossy())
    {
        return Err("Bot 数据目录不属于当前托管根".into());
    }
    let qq = Path::new(&spec.qq_executable);
    if !qq.is_absolute()
        || !qq.is_file()
        || qq
            .file_name()
            .is_none_or(|name| !name.to_string_lossy().eq_ignore_ascii_case("QQ.exe"))
    {
        return Err("目标 QQ.exe 不存在或路径无效".into());
    }
    let install = match spec.config.bot.backend_type {
        BackendType::NapCat => &spec.napcat_dir,
        BackendType::SnowLuma => &spec.snowluma_dir,
    };
    if !Path::new(install).is_absolute() || !Path::new(install).is_dir() {
        return Err("目标协议端安装目录不存在".into());
    }
    Ok(())
}

pub(super) async fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| error.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    let temporary = path.with_extension("json.pending");
    tokio::fs::write(&temporary, bytes)
        .await
        .map_err(|error| error.to_string())?;
    tokio::fs::rename(&temporary, path)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn check_intent(root: &Path, spec: &WindowsBotSpec) -> Result<(), String> {
    check_revision(root, &spec.bot_id, spec.intent_revision.as_deref()).await
}

async fn check_revision(root: &Path, bot_id: &str, expected: Option<&str>) -> Result<(), String> {
    let path = root
        .join("session-bots")
        .join(format!("{bot_id}.stop.json"));
    let revision = match tokio::fs::read(&path).await {
        Ok(bytes) => Some(
            serde_json::from_slice::<String>(&bytes)
                .map_err(|error| format!("停止记录无效：{error}"))?,
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.to_string()),
    };
    if revision.as_deref() != expected {
        return Err("启动请求已被更新的停止意图取消".into());
    }
    Ok(())
}

pub(super) async fn render(spec: &WindowsBotSpec) -> Result<(), String> {
    let bot_id = BotId::new(&spec.bot_id);
    let directory = match spec.config.bot.backend_type {
        BackendType::NapCat => Path::new(&spec.napcat_dir).join("config"),
        BackendType::SnowLuma => Path::new(&spec.snowluma_dir).join("config"),
    };
    let mut existing = HashMap::new();
    if let Ok(mut entries) = tokio::fs::read_dir(&directory).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name != format!("onebot11_{}.json", spec.bot_id)
                && name != format!("napcat_{}.json", spec.bot_id)
                && name != "napcat.json"
                && name != format!("onebot_{}.json", spec.bot_id)
            {
                continue;
            }
            let bytes = tokio::fs::read(entry.path())
                .await
                .map_err(|error| error.to_string())?;
            let payload = serde_json::from_slice(&bytes)
                .map_err(|error| format!("已有配置无效，未覆盖 {name}：{error}"))?;
            existing.insert(name, payload);
        }
    }
    let payloads = match spec.config.bot.backend_type {
        BackendType::NapCat => {
            render_napcat_docker_config_payloads(&bot_id, &spec.config, &existing)
        }
        BackendType::SnowLuma => {
            render_snowluma_docker_config_payloads(&bot_id, &spec.config, &existing)
        }
    };
    for payload in payloads {
        write_json(&directory.join(payload.file_name), &payload.payload).await?;
    }
    if spec.config.bot.backend_type == BackendType::SnowLuma {
        let data = Path::new(&spec.data_root).join("snowluma");
        let mut settings = ncd_backend_snowluma::snowluma::session::load_snowluma_app_config(&data);
        settings.webui_password_override = spec.webui_password.clone();
        settings.webui_port = spec.webui_port;
        settings.remote_qr_login_enabled = spec.remote_qr_login_enabled;
        settings.local_qr_login_enabled = spec.remote_qr_login_enabled;
        write_json(&data.join("app-config.json"), &settings).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_path_like_or_zero_bot_ids() {
        for id in ["", "0", "../123", "1/2", "-42", "18446744073709551616"] {
            assert!(valid_bot_id(id).is_err(), "{id}");
        }
        assert!(valid_bot_id("123456").is_ok());
    }

    #[tokio::test]
    async fn a_stop_cancels_in_flight_start_and_preserves_a_new_start() {
        let root = tempfile::tempdir().unwrap();
        assert!(check_revision(root.path(), "123", None).await.is_ok());
        let marker = root.path().join("session-bots/123.stop.json");
        write_json(&marker, &"stop-one").await.unwrap();
        assert!(check_revision(root.path(), "123", None).await.is_err());
        assert!(
            check_revision(root.path(), "123", Some("stop-one"))
                .await
                .is_ok()
        );
        write_json(&marker, &"stop-two").await.unwrap();
        assert!(
            check_revision(root.path(), "123", Some("stop-one"))
                .await
                .is_err()
        );
    }
}
