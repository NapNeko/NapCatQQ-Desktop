//! 识别已有 AstrBot 工作目录：可解析的 `data/cmd_config.json`。

use ncd_domain::AppProjectProbe;
use ncd_host::{Host, HostPath};
use ncd_traits::AppFrameworkError;

use super::config_json::{cmd_config_path, parse_cmd_config};
use super::manifest::{ASTRBOT_CMD_CONFIG, ASTRBOT_FRAMEWORK_ID};
use super::platform::{claim_for_probe, platforms, read_aiocqhttp_port};
use crate::uv_tooling::venv_script;

pub async fn probe_astrbot(
    host: &dyn Host,
    path: &HostPath,
) -> Result<AppProjectProbe, AppFrameworkError> {
    let cfg_path = cmd_config_path(path);
    if !host
        .exists(&cfg_path)
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?
    {
        return Err(AppFrameworkError::Validation(
            "目录里没有 data/cmd_config.json，不是 AstrBot 工作目录".into(),
        ));
    }
    let bytes = host
        .read_file(&cfg_path)
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    let root = parse_cmd_config(&String::from_utf8_lossy(&bytes))?;

    let mut warnings = Vec::new();
    let rows = platforms(&root);
    let claimed = claim_for_probe(rows);
    let port = claimed.and_then(|i| read_aiocqhttp_port(&rows[i]));
    let aiocqhttp_n = rows
        .iter()
        .filter(|r| super::platform::is_aiocqhttp(r))
        .count();
    if aiocqhttp_n == 0 {
        warnings.push("还没有 OneBot v11，对接时会加一条".into());
    } else if claimed.is_none() {
        warnings.push(super::platform::AMBIGUOUS_AIOCQHTTP.into());
    }

    let astrbot_bin = venv_script(path, host.os(), "astrbot");
    let ready = host
        .exists(&astrbot_bin)
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    if !ready {
        warnings.push("还没装依赖，导入后要先同步才能启动".into());
    }

    let display_name = path.file_name().unwrap_or("AstrBot").to_string();

    Ok(AppProjectProbe {
        framework_id: ncd_domain::AppFrameworkId::new(ASTRBOT_FRAMEWORK_ID),
        path: path.as_posix().to_string(),
        display_name,
        port,
        version: None,
        env_rel_path: ASTRBOT_CMD_CONFIG.to_string(),
        environment: String::new(),
        ready,
        running: false,
        supervisors: Vec::new(),
        warnings,
        detected_bot_id: None,
    })
}

#[cfg(test)]
mod tests {
    use super::super::platform::claim_for_probe;
    use super::super::platform::platforms;
    use serde_json::Value;

    #[test]
    fn probe_claim_follows_single_enabled() {
        let root: Value = serde_json::from_str(
            r#"{
              "platform": [
                { "id": "off", "type": "aiocqhttp", "enable": false, "ws_reverse_port": 1 },
                { "id": "on", "type": "aiocqhttp", "enable": true, "ws_reverse_port": 6199 }
              ]
            }"#,
        )
        .unwrap();
        assert_eq!(claim_for_probe(platforms(&root)), Some(1));
    }
}
