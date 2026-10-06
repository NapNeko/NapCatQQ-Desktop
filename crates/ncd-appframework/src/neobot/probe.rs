//! 识别已有 NeoBot 项目（`app/data/config.toml` + `[adapter]`）。
//!
//! 只有打包布局（`data/config.toml`）的目录不收：桌面端用 .venv 启动并把数据目录钉在
//! app/data，导入后原来的配置、模型和聊天记录一样都用不上，与其导进来当新实例跑，
//! 不如明说让用户先把 data/ 挪过去。
//!
//! 导入已有项目时只做识别，不改文件；同步依赖交给组件的 `adopt_provision`。

use ncd_domain::{AppFrameworkId, AppProjectProbe};
use ncd_host::{Host, HostPath};
use ncd_traits::AppFrameworkError;

use super::component::NeoBotComponent;
use super::config::read_neobot_config;
use super::manifest::{
    KEY_ADAPTER, NEOBOT_CONFIG_TOML, NEOBOT_CONFIG_TOML_LEGACY, NEOBOT_DASHBOARD_CONFIG,
    NEOBOT_DATA_DIR, NEOBOT_FRAMEWORK_ID,
};

/// config.toml 的样子够不够像 NeoBot。
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
    // 两份都在时以 app/data 为准：旁边的 data/ 多半是旧版桌面端误写的壳
    if !exists(host, path, NEOBOT_CONFIG_TOML).await? {
        if exists(host, path, NEOBOT_CONFIG_TOML_LEGACY).await? {
            return Err(AppFrameworkError::Validation(format!(
                "这个项目的数据在 {NEOBOT_CONFIG_TOML_LEGACY}（打包版的布局），桌面端启动的 NeoBot 只读 {NEOBOT_DATA_DIR}。先把 data 目录移到 {NEOBOT_DATA_DIR} 再导入"
            )));
        }
        return Err(AppFrameworkError::Validation(format!(
            "目录里没有 {NEOBOT_CONFIG_TOML}，不是 NeoBot 项目"
        )));
    }
    let text = read_text(host, &path.join(NEOBOT_CONFIG_TOML))
        .await?
        .unwrap_or_default();
    if !config_looks_like_neobot(&text) {
        return Err(AppFrameworkError::Validation(format!(
            "{NEOBOT_CONFIG_TOML} 里没有 [adapter] 分区，不像是 NeoBot 项目"
        )));
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
                    "反向 WS 没配 access token，NeoBot 不校验握手；对接时会给它写一个".to_string(),
                );
            }
        }
        Err(e) => {
            warnings.push(format!("读配置失败（{e}），端口要导入后自己核对"));
        }
    }

    // 面板配置可能不存在（面板从没起过），那不是错误
    if !host
        .exists(&path.join(NEOBOT_DASHBOARD_CONFIG))
        .await
        .unwrap_or(false)
    {
        warnings.push(format!(
            "还没有面板配置（{NEOBOT_DASHBOARD_CONFIG}），面板会按出厂值启动"
        ));
    }

    let comp = NeoBotComponent::new(path.clone(), port.unwrap_or(0), dashboard_port.unwrap_or(0));
    let ready = host
        .exists(&comp.neobot_bin(host.os()))
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    if !ready {
        warnings.push("还没装依赖（.venv 里没有 neobot），导入后要先同步才能启动".into());
    }

    let display_name = path.file_name().unwrap_or("NeoBot").to_string();

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

async fn exists(host: &dyn Host, root: &HostPath, rel: &str) -> Result<bool, AppFrameworkError> {
    host.exists(&root.join(rel))
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))
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
        assert!(config_looks_like_neobot("\n[adapter]\nmode = \"onebot\"\n"));
        assert!(config_looks_like_neobot("[bot]\n# neobot 的配置\n"));
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

    // ---- 数据目录布局：桌面端只读写 app/data ----

    use crate::config_backup::tests::TestHost;
    use std::sync::Arc;

    const CONFIG: &str = "[adapter]\nmode = \"onebot\"\nreverse_ws_port = 8085\n";

    fn host_with(files: &[(&str, &str)]) -> Arc<TestHost> {
        let host = TestHost::new("probe");
        for (p, body) in files {
            host.put(p, body.as_bytes());
        }
        host
    }

    #[tokio::test]
    async fn probe_accepts_the_pypi_nested_data_dir() {
        let host = host_with(&[("/n1/app/data/config.toml", CONFIG)]);
        let probe = probe_neobot(host.as_ref(), &HostPath::from_posix("/n1"))
            .await
            .expect("app/data 布局必须能导入");
        assert_eq!(probe.env_rel_path, "app/data/config.toml");
        assert_eq!(probe.port, Some(8085), "端口要从 app/data 那份读");
    }

    #[tokio::test]
    async fn probe_rejects_packaged_only_layout_with_a_hint() {
        // 导进来也用不上原数据：桌面端启动的 NeoBot 只读 app/data
        let host = host_with(&[("/n1/data/config.toml", CONFIG)]);
        let err = probe_neobot(host.as_ref(), &HostPath::from_posix("/n1"))
            .await
            .expect_err("只有 data/ 的项目不该被当成可直接导入");
        let msg = format!("{err}");
        assert!(
            msg.contains("data/config.toml") && msg.contains("app/data"),
            "{msg}"
        );
    }

    #[tokio::test]
    async fn probe_prefers_the_nested_layout_when_both_exist() {
        // 两套都在时以规范布局为准：桌面端就是按它读写的
        let host = host_with(&[
            ("/n1/data/config.toml", CONFIG),
            ("/n1/app/data/config.toml", CONFIG),
        ]);
        let probe = probe_neobot(host.as_ref(), &HostPath::from_posix("/n1"))
            .await
            .unwrap();
        assert_eq!(probe.env_rel_path, "app/data/config.toml");
    }

    #[tokio::test]
    async fn probe_rejects_a_directory_without_either_layout() {
        let host = TestHost::new("probe");
        let err = probe_neobot(host.as_ref(), &HostPath::from_posix("/n1"))
            .await
            .expect_err("两套布局都没有就不是 NeoBot 项目");
        assert!(format!("{err}").contains("不是 NeoBot 项目"), "{err}");
    }
}
