//! NeoBot 类型化配置：两份 TOML 的对接相关键。
//!
//! - `data/config.toml` 的 `[adapter]`：反向 WS 监听地址 / 口 / token（对接必需）
//! - `plugins_data/dashboard/config.toml`：面板监听地址 / 口（WebUI 口与 OneBot 口是两个口，
//!   桌面端要拿真实面板口去开隧道，读不到就会拿实例口去连、必然失败）
//!
//! 只声明**对接与开 WebUI 需要**的键，上游其余几百个配置键原样留在文件里不动
//! （差量写：`toml_patch::patch` 只改有变化的键，保注释与未知键）。
//!
//! 热加载：两份都吃热改。`data/config.toml` 改完 `adapter_supervisor` 会
//! stop → reconfigure → start 重建监听；面板配置由面板自己读。所以两份都标
//! `hot_reload = true`，改完不必重启实例。

use ncd_domain::{AppConfigDocument, AppConfigFormat, AppConfigIssue};
use ncd_host::{Host, HostPath};
use ncd_traits::AppFrameworkError;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::adapter::apply_with_backup_ex;
use crate::config_doc::{DocumentSnapshot, read_documents};
use crate::toml_patch;

use super::manifest::{
    ADAPTER_MODE_ONEBOT, KEY_ADAPTER, KEY_ADAPTER_MODE, KEY_DASHBOARD_HOST, KEY_DASHBOARD_PORT,
    KEY_REVERSE_WS_ACCESS_TOKEN, KEY_REVERSE_WS_HOST, KEY_REVERSE_WS_PORT, NEOBOT_CONFIG_TOML,
    NEOBOT_DASHBOARD_CONFIG, NEOBOT_DEFAULT_DASHBOARD_PORT, NEOBOT_DEFAULT_ONEBOT_PORT,
};

pub const DOC_ADAPTER: &str = "adapter";
pub const DOC_DASHBOARD: &str = "dashboard";

/// 协议 Bot 连过来的反向 WS 设置（`data/config.toml` 的 `[adapter]`）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct NeoBotAdapterConfig {
    /// onebot 或 local；只有 onebot 才有反向 WS 监听
    pub mode: String,
    pub reverse_ws_host: String,
    pub reverse_ws_port: u16,
    pub reverse_ws_access_token: String,
}

impl Default for NeoBotAdapterConfig {
    fn default() -> Self {
        Self {
            mode: ADAPTER_MODE_ONEBOT.to_string(),
            // 桌面端实例的默认监听地址：协议 Bot 与实例同机走回环，
            // 跨机由编排层开隧道。上游出厂值是空串（= 0.0.0.0），这里不跟，
            // 免得新实例默认对全网开放。
            reverse_ws_host: "127.0.0.1".to_string(),
            reverse_ws_port: NEOBOT_DEFAULT_ONEBOT_PORT,
            reverse_ws_access_token: String::new(),
        }
    }
}

/// 网页面板设置（`plugins_data/dashboard/config.toml`，平铺键）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct NeoBotDashboardConfig {
    pub host: String,
    pub port: u16,
}

impl Default for NeoBotDashboardConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: NEOBOT_DEFAULT_DASHBOARD_PORT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct NeoBotInstanceConfig {
    pub adapter: NeoBotAdapterConfig,
    pub dashboard: NeoBotDashboardConfig,
}

impl Default for NeoBotInstanceConfig {
    fn default() -> Self {
        Self {
            adapter: NeoBotAdapterConfig::default(),
            dashboard: NeoBotDashboardConfig::default(),
        }
    }
}

impl NeoBotInstanceConfig {
    /// 对接依赖的输入：口或 token 变了要重新对接
    pub fn link_inputs_changed(&self, after: &Self) -> bool {
        self.adapter.reverse_ws_port != after.adapter.reverse_ws_port
            || self.adapter.reverse_ws_host != after.adapter.reverse_ws_host
            || self.adapter.reverse_ws_access_token != after.adapter.reverse_ws_access_token
            || self.adapter.mode != after.adapter.mode
    }

    /// 面板口变了要重开隧道；OneBot 口变了要重启监听
    pub fn restart_inputs_changed(&self, after: &Self) -> bool {
        self.adapter.reverse_ws_port != after.adapter.reverse_ws_port
            || self.dashboard.port != after.dashboard.port
    }

    pub fn validate(&self) -> Vec<AppConfigIssue> {
        let mut issues = Vec::new();
        if self.adapter.reverse_ws_port == 0 {
            issues.push(AppConfigIssue {
                path: "adapter/reverse_ws_port".to_string(),
                message: "OneBot 反向 WS 端口不能为 0".to_string(),
            });
        }
        if self.dashboard.port == 0 {
            issues.push(AppConfigIssue {
                path: "dashboard/port".to_string(),
                message: "面板端口不能为 0".to_string(),
            });
        }
        if self.adapter.reverse_ws_port != 0
            && self.adapter.reverse_ws_port == self.dashboard.port
        {
            issues.push(AppConfigIssue {
                path: "dashboard/port".to_string(),
                message: format!(
                    "面板口不能与 OneBot 口相同（都是 {}）",
                    self.dashboard.port
                ),
            });
        }
        if self.adapter.mode.trim() != ADAPTER_MODE_ONEBOT {
            issues.push(AppConfigIssue {
                path: "adapter/mode".to_string(),
                message: format!(
                    "只有 \"{ADAPTER_MODE_ONEBOT}\" 模式才有反向 WS 监听（现在是 {:?}）",
                    self.adapter.mode
                ),
            });
        }
        if self.adapter.reverse_ws_host.trim().is_empty() {
            issues.push(AppConfigIssue {
                path: "adapter/reverse_ws_host".to_string(),
                message: "反向 WS 监听地址不能为空（留空会落到 0.0.0.0）".to_string(),
            });
        }
        if self.dashboard.host.trim().is_empty() {
            issues.push(AppConfigIssue {
                path: "dashboard/host".to_string(),
                message: "面板监听地址不能为空（留空会落到 0.0.0.0）".to_string(),
            });
        }
        issues
    }
}

pub fn neobot_config_documents() -> Vec<AppConfigDocument> {
    vec![
        config_doc(DOC_ADAPTER, NEOBOT_CONFIG_TOML, AppConfigFormat::Toml),
        config_doc(
            DOC_DASHBOARD,
            NEOBOT_DASHBOARD_CONFIG,
            AppConfigFormat::Toml,
        ),
    ]
}

fn config_doc(id: &str, rel: &str, format: AppConfigFormat) -> AppConfigDocument {
    AppConfigDocument {
        id: id.to_string(),
        label: match id {
            DOC_ADAPTER => "OneBot 对接（data/config.toml）".to_string(),
            _ => "网页面板（plugins_data/dashboard/config.toml）".to_string(),
        },
        rel_path: rel.to_string(),
        format,
        // 两份都吃热改：adapter 由 adapter_supervisor 重建监听，面板自己读
        hot_reload: true,
    }
}

fn parse_table(text: Option<&str>) -> Result<toml::Table, AppFrameworkError> {
    match text {
        None => Ok(toml::Table::new()),
        Some(t) if t.trim().is_empty() => Ok(toml::Table::new()),
        Some(t) => t
            .parse::<toml::Table>()
            .map_err(|e| AppFrameworkError::ConfigInvalid(vec![AppConfigIssue {
                path: String::new(),
                message: format!("TOML 解析失败：{e}"),
            }])),
    }
}

pub fn parse_neobot_config(
    snaps: &[DocumentSnapshot],
) -> Result<NeoBotInstanceConfig, AppFrameworkError> {
    let adapter_text = snaps
        .iter()
        .find(|s| s.doc.id == DOC_ADAPTER)
        .and_then(|s| s.text.as_deref());
    let dashboard_text = snaps
        .iter()
        .find(|s| s.doc.id == DOC_DASHBOARD)
        .and_then(|s| s.text.as_deref());
    if adapter_text.is_none() && dashboard_text.is_none() {
        return Err(AppFrameworkError::Integration(
            "NeoBot 实例没有 data/config.toml 也没有面板配置".to_string(),
        ));
    }
    let adapter_table = parse_table(adapter_text)?;
    let dashboard_table = parse_table(dashboard_text)?;

    let adapter_tbl = adapter_table.get(KEY_ADAPTER).and_then(|v| v.as_table());
    let mut adapter = NeoBotAdapterConfig::default();
    if let Some(t) = adapter_tbl {
        // 键在就采信（值本身可能是空串），只有类型不对才当没读到
        if let Some(s) = t.get(KEY_ADAPTER_MODE).and_then(|v| v.as_str()) {
            adapter.mode = s.to_string();
        }
        if let Some(s) = t.get(KEY_REVERSE_WS_HOST).and_then(|v| v.as_str()) {
            adapter.reverse_ws_host = s.to_string();
        }
        if let Some(p) = t
            .get(KEY_REVERSE_WS_PORT)
            .and_then(|v| v.as_integer())
            .and_then(|p| u16::try_from(p).ok())
            .filter(|p| *p > 0)
        {
            adapter.reverse_ws_port = p;
        }
        if let Some(v) = t
            .get(KEY_REVERSE_WS_ACCESS_TOKEN)
            .and_then(|v| v.as_str())
        {
            adapter.reverse_ws_access_token = v.to_string();
        }
    }
    // 上游语义：reverse_ws_host 留空 = 落到 0.0.0.0。
    // 文件里显式写了空串时按上游解释，而不是当校验错误——那是用户/上游的合法配置。
    if adapter.reverse_ws_host.trim().is_empty() {
        adapter.reverse_ws_host = "0.0.0.0".to_string();
    }

    let mut dashboard = NeoBotDashboardConfig::default();
    if let Some(v) = dashboard_table
        .get(KEY_DASHBOARD_HOST)
        .and_then(|v| v.as_str())
    {
        dashboard.host = v.to_string();
    }
    if let Some(p) = dashboard_table
        .get(KEY_DASHBOARD_PORT)
        .and_then(|v| v.as_integer())
        .and_then(|p| u16::try_from(p).ok())
        .filter(|p| *p > 0)
    {
        dashboard.port = p;
    }

    Ok(NeoBotInstanceConfig {
        adapter,
        dashboard,
    })
}

pub async fn read_neobot_config(
    host: &dyn Host,
    install_dir: &HostPath,
) -> Result<(NeoBotInstanceConfig, Vec<DocumentSnapshot>), AppFrameworkError> {
    let snaps = read_documents(host, install_dir, &neobot_config_documents()).await?;
    parse_neobot_config(&snaps).map(|config| (config, snaps))
}

pub async fn write_neobot_config(
    host: &dyn Host,
    install_dir: &HostPath,
    config: &NeoBotInstanceConfig,
    write_sidecar: bool,
) -> Result<(NeoBotInstanceConfig, Vec<DocumentSnapshot>), AppFrameworkError> {
    write_config_docs(host, install_dir, config, write_sidecar, true).await
}

/// 对接（apply_link）路径的写入：面板配置只在文件已存在时写。受管实例首装已
/// seed 面板文件，照常差量更新；领养项目没装面板插件时不替它新建
/// `plugins_data/dashboard/config.toml`——新建的文件 rollback 还原不掉，属污染用户目录。
pub async fn write_link_config(
    host: &dyn Host,
    install_dir: &HostPath,
    config: &NeoBotInstanceConfig,
    write_sidecar: bool,
) -> Result<(NeoBotInstanceConfig, Vec<DocumentSnapshot>), AppFrameworkError> {
    write_config_docs(host, install_dir, config, write_sidecar, false).await
}

async fn write_config_docs(
    host: &dyn Host,
    install_dir: &HostPath,
    config: &NeoBotInstanceConfig,
    write_sidecar: bool,
    create_dashboard: bool,
) -> Result<(NeoBotInstanceConfig, Vec<DocumentSnapshot>), AppFrameworkError> {
    let issues = config.validate();
    if !issues.is_empty() {
        return Err(AppFrameworkError::ConfigInvalid(issues));
    }
    write_one(
        host,
        &install_dir.join(NEOBOT_CONFIG_TOML),
        &adapter_patch(config),
        write_sidecar,
    )
    .await?;
    let dashboard_path = install_dir.join(NEOBOT_DASHBOARD_CONFIG);
    let dashboard_writable = create_dashboard
        || host
            .exists(&dashboard_path)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    if dashboard_writable {
        write_one(host, &dashboard_path, &dashboard_patch(config), write_sidecar).await?;
    }
    read_neobot_config(host, install_dir).await
}

/// 首装后补 `[adapter].reverse_ws_access_token` 键（空串占位，正式 token 由对接写）。
/// 只补缺失键：键已存在就什么都不做；写入走差量，保住注释与键序。
/// 文件不存在或没有 `[adapter]` 表时不新建——那不是首装该有的形状。
pub async fn ensure_token_key(
    host: &dyn Host,
    install_dir: &HostPath,
    write_sidecar: bool,
) -> Result<(), AppFrameworkError> {
    let path = install_dir.join(NEOBOT_CONFIG_TOML);
    let Some(text) = read_optional_text(host, &path).await? else {
        return Ok(());
    };
    let table = parse_table(Some(&text))?;
    let needs_token = table
        .get(KEY_ADAPTER)
        .and_then(|v| v.as_table())
        .is_some_and(|t| !t.contains_key(KEY_REVERSE_WS_ACCESS_TOKEN));
    if !needs_token {
        return Ok(());
    }
    let mut token = toml::Table::new();
    token.insert(
        KEY_REVERSE_WS_ACCESS_TOKEN.to_string(),
        toml::Value::String(String::new()),
    );
    let mut patch = toml::Table::new();
    patch.insert(KEY_ADAPTER.to_string(), toml::Value::Table(token));
    write_one(host, &path, &patch, write_sidecar).await
}

/// `[adapter]` 的期望值（差量写只动这几个键）
fn adapter_patch(config: &NeoBotInstanceConfig) -> toml::Table {
    let mut adapter = toml::Table::new();
    adapter.insert(
        KEY_ADAPTER_MODE.to_string(),
        toml::Value::String(config.adapter.mode.clone()),
    );
    adapter.insert(
        KEY_REVERSE_WS_HOST.to_string(),
        toml::Value::String(config.adapter.reverse_ws_host.clone()),
    );
    adapter.insert(
        KEY_REVERSE_WS_PORT.to_string(),
        toml::Value::Integer(i64::from(config.adapter.reverse_ws_port)),
    );
    adapter.insert(
        KEY_REVERSE_WS_ACCESS_TOKEN.to_string(),
        toml::Value::String(config.adapter.reverse_ws_access_token.clone()),
    );
    let mut root = toml::Table::new();
    root.insert(KEY_ADAPTER.to_string(), toml::Value::Table(adapter));
    root
}

/// 面板配置是平铺表，没有子表
fn dashboard_patch(config: &NeoBotInstanceConfig) -> toml::Table {
    let mut root = toml::Table::new();
    root.insert(
        KEY_DASHBOARD_HOST.to_string(),
        toml::Value::String(config.dashboard.host.clone()),
    );
    root.insert(
        KEY_DASHBOARD_PORT.to_string(),
        toml::Value::Integer(i64::from(config.dashboard.port)),
    );
    root
}

/// 备份 → 差量写 → 失败还原。内容没变就不写（少一次 mtime 变动）
async fn write_one(
    host: &dyn Host,
    path: &HostPath,
    patch: &toml::Table,
    write_sidecar: bool,
) -> Result<(), AppFrameworkError> {
    let original = read_optional_text(host, path).await?.unwrap_or_default();
    let before = parse_table(Some(&original))?;
    let mut after = before.clone();
    merge_tables(&mut after, patch);
    // 没有改动就不落盘：免得白动一次 mtime，也免得把纯注释差异写成内容变更
    if toml_patch::changes(&before, &after).is_empty() {
        return Ok(());
    }
    let mut doc = original
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| AppFrameworkError::Integration(format!("{} 解析失败：{e}", path.as_posix())))?;
    // 差量写：只动变了的地方，保住注释、键序和 Desktop 不认识的键
    toml_patch::apply(&mut doc, &before, &after, &toml_patch::no_identity);
    let out = doc.to_string();
    apply_with_backup_ex(host, std::slice::from_ref(path), write_sidecar, || async {
        host.write_file(path, out.as_bytes())
            .await
            .map_err(|e| AppFrameworkError::Integration(e.to_string()))
    })
    .await
}

/// 把 `patch` 覆盖到 `base` 上（同名表递归合并，叶子整值替换）
fn merge_tables(base: &mut toml::Table, patch: &toml::Table) {
    for (key, value) in patch {
        match (base.get_mut(key), value) {
            (Some(toml::Value::Table(existing)), toml::Value::Table(incoming)) => {
                merge_tables(existing, incoming);
            }
            _ => {
                base.insert(key.clone(), value.clone());
            }
        }
    }
}

pub async fn read_optional_text(
    host: &dyn Host,
    path: &HostPath,
) -> Result<Option<String>, AppFrameworkError> {
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

    fn snap(id: &str, text: &str) -> DocumentSnapshot {
        let doc = neobot_config_documents()
            .into_iter()
            .find(|d| d.id == id)
            .unwrap();
        DocumentSnapshot {
            doc,
            text: Some(text.to_string()),
            revision: "r".to_string(),
        }
    }

    #[test]
    fn parses_upstream_shaped_config() {
        let snaps = vec![
            snap(
                DOC_ADAPTER,
                "\n[adapter]\nmode = \"onebot\"\nreverse_ws_host = \"127.0.0.1\"\nreverse_ws_port = 8080\nreverse_ws_access_token = \"tok\"\n",
            ),
            snap(DOC_DASHBOARD, "host = \"127.0.0.1\"\nport = 9981\n"),
        ];
        let cfg = parse_neobot_config(&snaps).unwrap();
        assert_eq!(cfg.adapter.mode, "onebot");
        assert_eq!(cfg.adapter.reverse_ws_port, 8080);
        assert_eq!(cfg.adapter.reverse_ws_access_token, "tok");
        assert_eq!(cfg.dashboard.port, 9981);
        assert_eq!(cfg.dashboard.host, "127.0.0.1");
    }

    #[test]
    fn missing_keys_fall_back_to_defaults() {
        let snaps = vec![snap(DOC_ADAPTER, "[adapter]\n"), snap(DOC_DASHBOARD, "")];
        let cfg = parse_neobot_config(&snaps).unwrap();
        assert_eq!(cfg.adapter.mode, "onebot", "缺 mode 按上游默认 onebot");
        assert_eq!(cfg.adapter.reverse_ws_port, 8080);
        assert_eq!(
            cfg.adapter.reverse_ws_host, "127.0.0.1",
            "键缺失时用桌面端默认（回环）；跟上游空串=0.0.0.0 是两回事，见下一个用例"
        );
        assert_eq!(cfg.dashboard.port, 9981);
    }

    /// 上游语义：文件里**显式**写空串 = 落到 0.0.0.0。这是合法配置，不是校验错误。
    /// 与「键缺失」区分开：缺失用桌面端默认（回环），显式空串按上游解释。
    #[test]
    fn explicit_empty_host_means_upstream_wildcard() {
        let snaps = vec![
            snap(DOC_ADAPTER, "[adapter]\nreverse_ws_host = \"\"\n"),
            snap(DOC_DASHBOARD, ""),
        ];
        let cfg = parse_neobot_config(&snaps).unwrap();
        assert_eq!(cfg.adapter.reverse_ws_host, "0.0.0.0");
        assert!(cfg.validate().is_empty(), "这是上游合法配置，不该报错");
    }

    #[test]
    fn empty_files_are_rejected() {
        let snaps = vec![
            DocumentSnapshot {
                doc: neobot_config_documents()[0].clone(),
                text: None,
                revision: "missing".to_string(),
            },
            DocumentSnapshot {
                doc: neobot_config_documents()[1].clone(),
                text: None,
                revision: "missing".to_string(),
            },
        ];
        assert!(matches!(
            parse_neobot_config(&snaps),
            Err(AppFrameworkError::Integration(_))
        ));
    }

    #[test]
    fn validate_rejects_zero_and_collision() {
        let mut cfg = NeoBotInstanceConfig::default();
        assert!(cfg.validate().is_empty());

        cfg.adapter.reverse_ws_port = 0;
        let issues = cfg.validate();
        assert!(issues.iter().any(|i| i.path == "adapter/reverse_ws_port"));

        let mut cfg = NeoBotInstanceConfig::default();
        cfg.dashboard.port = cfg.adapter.reverse_ws_port;
        let issues = cfg.validate();
        assert!(
            issues.iter().any(|i| i.path == "dashboard/port"),
            "面板口与 OneBot 口相同要报"
        );

        let mut cfg = NeoBotInstanceConfig::default();
        cfg.adapter.mode = "local".to_string();
        let issues = cfg.validate();
        assert!(issues.iter().any(|i| i.path == "adapter/mode"));
    }

    #[test]
    fn patch_only_touches_known_keys() {
        let existing: toml::Table = "\n[adapter]\nmode = \"onebot\"\nlocal_port = 8090\nreverse_ws_port = 1\n\n[bot]\nqq = 10001\n"
            .parse()
            .unwrap();
        let cfg = NeoBotInstanceConfig::default();
        let mut patched = existing.clone();
        merge_tables(&mut patched, &adapter_patch(&cfg));
        let adapter = patched.get(KEY_ADAPTER).and_then(|v| v.as_table()).unwrap();
        assert_eq!(
            adapter.get("local_port").and_then(|v| v.as_integer()),
            Some(8090),
            "不认识的键要留着"
        );
        assert_eq!(
            adapter.get(KEY_REVERSE_WS_PORT).and_then(|v| v.as_integer()),
            Some(8080)
        );
        assert_eq!(
            patched
                .get("bot")
                .and_then(|v| v.get("qq"))
                .and_then(|v| v.as_integer()),
            Some(10001),
            "别的分区不能被碰"
        );
    }

    #[test]
    fn link_and_restart_inputs_changes() {
        let before = NeoBotInstanceConfig::default();
        let mut after = before.clone();
        after.adapter.reverse_ws_access_token = "new".to_string();
        assert!(before.link_inputs_changed(&after));
        assert!(!before.restart_inputs_changed(&after));

        let mut port_change = before.clone();
        port_change.adapter.reverse_ws_port = 9090;
        assert!(before.link_inputs_changed(&port_change));
        assert!(before.restart_inputs_changed(&port_change));

        let mut dash = before.clone();
        dash.dashboard.port = 9999;
        assert!(!before.link_inputs_changed(&dash));
        assert!(before.restart_inputs_changed(&dash), "换面板口要重开隧道");
    }

    #[test]
    fn documents_cover_both_files() {
        let docs = neobot_config_documents();
        assert_eq!(docs.len(), 2);
        assert_eq!(docs[0].rel_path, "data/config.toml");
        assert_eq!(docs[1].rel_path, "plugins_data/dashboard/config.toml");
        assert!(docs.iter().all(|d| d.format == AppConfigFormat::Toml));
        assert!(docs.iter().all(|d| d.hot_reload));
    }
}
