//! 识别已有 NeoBot 项目（数据目录里的 config.toml + `[adapter]`）。
//!
//! **两套数据目录布局都要认**（导入已有项目曾因为只认一套而失败）：
//! - `app/data/`：PyPI / 源码安装。NeoBot 的 get_data_dir() 在非打包运行时返回
//!   「项目根/app/data」（没有 .git / pyproject.toml 祖先时回落到「cwd/app/data」），
//!   而桌面端就是以实例目录为 cwd 启动它的 —— 所以这才是最常见的布局，也是桌面端读写的那套；
//! - `data/`：打包运行（exe 目录/data）。
//!
//! 只找到打包布局时不拦（那确实是个 NeoBot 项目），但会提醒两边读写位置不同。
//!
//! 导入已有项目时只做识别，不改文件；同步依赖交给组件的 `adopt_provision`。

use ncd_domain::{AppProjectProbe, AppFrameworkId};
use ncd_host::{Host, HostPath};
use ncd_traits::AppFrameworkError;

use super::component::NeoBotComponent;
use super::config::read_neobot_config;
use super::manifest::{
    KEY_ADAPTER, NEOBOT_CONFIG_TOML, NEOBOT_CONFIG_TOML_LEGACY, NEOBOT_DASHBOARD_CONFIG,
    NEOBOT_DASHBOARD_CONFIG_LEGACY, NEOBOT_DATA_DIR, NEOBOT_FRAMEWORK_ID,
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
    // 先认规范布局（app/data），再认打包布局（data）。两套都可能存在（例如桌面端
    // 按 app/data 建的实例旁边还留着早期误写的 data/），规范布局优先。
    let mut found: Option<(&str, bool)> = None;
    for (rel, canonical) in [
        (NEOBOT_CONFIG_TOML, true),
        (NEOBOT_CONFIG_TOML_LEGACY, false),
    ] {
        if host
            .exists(&path.join(rel))
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?
        {
            found = Some((rel, canonical));
            break;
        }
    }
    let Some((config_rel, canonical)) = found else {
        return Err(AppFrameworkError::Validation(
            "目录里既没有 app/data/config.toml 也没有 data/config.toml，不是 NeoBot 项目".into(),
        ));
    };
    let config = path.join(config_rel);
    let text = read_text(host, &config).await?.unwrap_or_default();
    if !config_looks_like_neobot(&text) {
        return Err(AppFrameworkError::Validation(format!(
            "{config_rel} 里没有 [adapter] 分区，不像是 NeoBot 项目"
        )));
    }

    let mut warnings = Vec::new();
    if !canonical {
        warnings.push(format!(
            "这个项目的数据在 {NEOBOT_CONFIG_TOML_LEGACY}（打包运行的布局）。桌面端按 {NEOBOT_DATA_DIR} 读写，导入后配置可能对不上，启动前先确认"
        ));
    }
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
    let dashboard_rel = if canonical {
        NEOBOT_DASHBOARD_CONFIG
    } else {
        NEOBOT_DASHBOARD_CONFIG_LEGACY
    };
    if !host.exists(&path.join(dashboard_rel)).await.unwrap_or(false) {
        warnings.push(format!(
            "还没有面板配置（{dashboard_rel}），面板会按出厂值启动"
        ));
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

    let display_name = path.file_name().unwrap_or("NeoBot").to_string();

    Ok(AppProjectProbe {
        framework_id: AppFrameworkId::new(NEOBOT_FRAMEWORK_ID),
        path: path.as_posix().to_string(),
        display_name,
        port,
        version: None,
        env_rel_path: config_rel.to_string(),
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

    // ---- 两套数据目录布局（实测反馈：PyPI 安装下只认 data/ 会导致导入失败）----

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
        // NeoBot 非打包运行时数据在 app/data（它按 cwd 算），这是最常见的布局
        let host = host_with(&[("/n1/app/data/config.toml", CONFIG)]);
        let probe = probe_neobot(host.as_ref(), &HostPath::from_posix("/n1"))
            .await
            .expect("app/data 布局必须能导入");
        assert_eq!(probe.env_rel_path, "app/data/config.toml");
        assert!(
            !probe.warnings.iter().any(|w| w.contains("打包运行")),
            "规范布局不该报布局警告：{:?}",
            probe.warnings
        );
    }

    #[tokio::test]
    async fn probe_still_accepts_the_packaged_flat_dir_but_warns() {
        let host = host_with(&[("/n1/data/config.toml", CONFIG)]);
        let probe = probe_neobot(host.as_ref(), &HostPath::from_posix("/n1"))
            .await
            .expect("打包布局也要能导入");
        assert_eq!(probe.env_rel_path, "data/config.toml");
        assert!(
            probe.warnings.iter().any(|w| w.contains("打包运行")),
            "读写位置不同必须提醒：{:?}",
            probe.warnings
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
