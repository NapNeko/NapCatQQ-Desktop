//! 识别已有 NeoBot 项目（`data/config.toml` + `[adapter]`）。
//!
//! 导入已有项目时只做识别，不改文件；同步依赖交给组件的 `adopt_provision`。

use ncd_domain::{AppProjectProbe, AppFrameworkId};
use ncd_host::{Host, HostPath};
use ncd_traits::AppFrameworkError;

use super::component::NeoBotComponent;
use super::config::read_neobot_config;
use super::manifest::{
    KEY_ADAPTER, NEOBOT_CONFIG_TOML, NEOBOT_DATA_DIR, NEOBOT_DASHBOARD_CONFIG, NEOBOT_FRAMEWORK_ID,
};

/// `data/config.toml` 的样子够不够像 NeoBot。
///
/// 先看能不能解析出 `[adapter]`（最可靠）；解析得了但没有 `[adapter]`、
/// 或者根本解析不了（用户手写坏了一半）时退到文本匹配：
/// 提到 `neobot` 或写了 `[adapter]` 就算，宁可让人导入后再核对，也别把真项目判死。
pub fn config_looks_like_neobot(text: &str) -> bool {
    if let Ok(table) = text.parse::<toml::Table>()
        && table.contains_key(KEY_ADAPTER)
    {
        return true;
    }
    let lower = text.to_ascii_lowercase();
    lower.contains("[adapter]") || lower.contains("neobot")
}

pub async fn probe_neobot(
    host: &dyn Host,
    path: &HostPath,
) -> Result<AppProjectProbe, AppFrameworkError> {
    let config = path.join(NEOBOT_CONFIG_TOML);
    if !host
        .exists(&config)
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?
    {
        return Err(AppFrameworkError::Validation(
            "目录里没有 data/config.toml，不是 NeoBot 项目".into(),
        ));
    }
    let text = read_text(host, &config).await?.unwrap_or_default();
    if !config_looks_like_neobot(&text) {
        return Err(AppFrameworkError::Validation(
            "data/config.toml 里没有 [adapter] 分区，不像是 NeoBot 项目".into(),
        ));
    }

    let mut warnings = Vec::new();
    let mut port = None;
    let mut dashboard_port = None;
    match read_neobot_config(host, path).await {
        Ok((cfg, _)) => {
            port = Some(cfg.adapter.reverse_ws_port).filter(|p| *p > 0);
            dashboard_port = Some(cfg.dashboard.port).filter(|p| *p > 0);
            if cfg.adapter.mode.trim() != "onebot" {
                warnings.push(format!(
                    "适配器模式是 {:?}，不是 onebot：反向 WS 不会监听，对接前要改回来",
                    cfg.adapter.mode
                ));
            }
            if cfg.adapter.reverse_ws_access_token.trim().is_empty() {
                warnings.push(
                    "反向 WS 没配 access token，NeoBot 不校验握手；对接时会给它写一个"
                        .to_string(),
                );
            }
        }
        Err(e) => {
            warnings.push(format!("读配置失败（{e}），端口要导入后自己核对"));
        }
    }

    // 面板配置可能不存在（面板从没起过），那不是错误
    let dashboard_config = path.join(NEOBOT_DASHBOARD_CONFIG);
    if !host.exists(&dashboard_config).await.unwrap_or(false) {
        warnings.push("还没有面板配置（plugins_data/dashboard/config.toml），面板会按出厂值启动".to_string());
    }

    let comp = NeoBotComponent::new(
        path.clone(),
        port.unwrap_or(0),
        dashboard_port.unwrap_or(0),
    );
    let ready = host
        .exists(&comp.neobot_bin(host.os()))
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    if !ready {
        warnings.push("还没装依赖（.venv 里没有 neobot），导入后要先同步才能启动".into());
    }

    let display_name = path
        .file_name()
        .unwrap_or("NeoBot")
        .to_string();

    Ok(AppProjectProbe {
        framework_id: AppFrameworkId::new(NEOBOT_FRAMEWORK_ID),
        path: path.as_posix().to_string(),
        display_name,
        port,
        version: None,
        env_rel_path: NEOBOT_CONFIG_TOML.to_string(),
        // NeoBot 没有 NoneBot 那种 ENVIRONMENT 分层；这里给数据目录名，
        // 只用于 UI 展示，不代表环境切换
        environment: NEOBOT_DATA_DIR.to_string(),
        ready,
        running: false,
        supervisors: Vec::new(),
        warnings,
        detected_bot_id: None,
    })
}

async fn read_text(host: &dyn Host, path: &HostPath) -> Result<Option<String>, AppFrameworkError> {
    if !host
        .exists(path)
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?
    {
        return Ok(None);
    }
    let bytes = host
        .read_file(path)
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_neobot_config_shapes() {
        assert!(config_looks_like_neobot(
            "\n[adapter]\nmode = \"onebot\"\n"
        ));
        assert!(config_looks_like_neobot(
            "[bot]\n# neobot 的配置\n"
        ));
        assert!(!config_looks_like_neobot("[bot]\nqq = 1\n"));
        assert!(!config_looks_like_neobot(""));
    }

    #[test]
    fn broken_toml_falls_back_to_text_match() {
        // 解析不了也不能直接判否：用户手写坏了一半时仍应认出来，让人导入后再修
        assert!(config_looks_like_neobot(
            "[adapter]\nmode = \"onebot\"\nbad_line_without_equals\n"
        ));
        // 真正的 [adapter] 表头是 NeoBot 的特征；别的框架的 config.toml 不会长这样
        assert!(!config_looks_like_neobot("[bot]\nqq = 1\n"));
    }
}
