//! Karin 官方插件目录条目、扫描与 Host 侧装更卸（HTTP 在 ncd-runtime）。

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use ncd_domain::{AppConfigDocument, AppConfigFormat, AppInstance};
use ncd_host::{Host, HostCommand, HostError, HostPath, Locality};
use ncd_traits::AppFrameworkError;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 装更卸命令行输出；编排层转成任务 `ProgressKind::Log`，这里不碰 DeploymentTask。
pub type PluginLogSink = Arc<dyn Fn(String) + Send + Sync>;

use super::config::{KARIN_CONFIG_DIR, KarinInstanceConfig, KarinScopeRule};
use crate::node_tooling::{
    local_path_env, path_prefix, pnpm_command, read_node_marker, resolve_node_toolchain,
};

const PLUGIN_CMD_TIMEOUT: Duration = Duration::from_secs(20 * 60);
const APP_PLUGIN_DIR: &str = "plugins/karin-plugin-example";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum KarinPluginKind {
    Npm,
    Git,
    App,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct KarinPluginAuthor {
    pub name: String,
    #[serde(default)]
    pub home: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct KarinPluginRepo {
    pub url: String,
    #[serde(rename = "type", default)]
    pub r#type: String,
    #[serde(default)]
    pub branch: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct KarinPluginAppFile {
    pub url: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct KarinPluginMarketEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: KarinPluginKind,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub time: String,
    #[serde(default)]
    pub home: String,
    #[serde(default, rename = "author")]
    pub authors: Vec<KarinPluginAuthor>,
    #[serde(default, rename = "repo")]
    pub repos: Vec<KarinPluginRepo>,
    #[serde(default)]
    pub files: Vec<KarinPluginAppFile>,
    #[serde(default, rename = "allowBuild")]
    pub allow_build: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct KarinPluginInstalled {
    pub name: String,
    pub kind: KarinPluginKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub version: Option<String>,
    pub enabled: bool,
}

pub fn parse_karin_plugins_list(json: &str) -> Result<Vec<KarinPluginMarketEntry>, AppFrameworkError> {
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| AppFrameworkError::Validation(format!("插件目录 JSON 无效: {e}")))?;
    let Some(plugins) = value.get("plugins") else {
        return Err(AppFrameworkError::Validation(
            "插件目录缺少 plugins".to_string(),
        ));
    };
    serde_json::from_value(plugins.clone())
        .map_err(|e| AppFrameworkError::Validation(format!("插件目录 plugins 解析失败: {e}")))
}

pub fn git_clone_url(entry: &KarinPluginMarketEntry) -> Option<&str> {
    entry
        .repos
        .iter()
        .find(|r| r.r#type == "github")
        .or_else(|| entry.repos.first())
        .map(|r| r.url.as_str())
}

/// 去掉 query 后必须是 `.js` / `.ts`；返回文件名（防路径穿越）。
pub fn app_file_basename(url: &str) -> Result<String, AppFrameworkError> {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let name = path
        .rsplit('/')
        .next()
        .unwrap_or("")
        .trim();
    if name.is_empty() || name.contains("..") || name.contains('\\') {
        return Err(AppFrameworkError::Validation(format!(
            "插件文件 URL 非法: {url}"
        )));
    }
    let lower = name.to_ascii_lowercase();
    if !lower.ends_with(".js") && !lower.ends_with(".ts") {
        return Err(AppFrameworkError::Validation(format!(
            "app 插件文件须为 .js / .ts: {url}"
        )));
    }
    Ok(name.to_string())
}

pub fn apply_plugin_enabled(
    cfg: &mut KarinInstanceConfig,
    name: &str,
    enabled: bool,
) -> Result<(), AppFrameworkError> {
    let has_group = cfg.groups.iter().any(|r| r.key == "default");
    let has_private = cfg.privates.iter().any(|r| r.key == "default");
    if !has_group || !has_private {
        return Err(AppFrameworkError::Validation("缺少 default 规则".into()));
    }
    for rule in cfg.groups.iter_mut().filter(|r| r.key == "default") {
        set_disable(&mut rule.disable, name, enabled);
    }
    for rule in cfg.privates.iter_mut().filter(|r| r.key == "default") {
        set_disable(&mut rule.disable, name, enabled);
    }
    Ok(())
}

fn set_disable(list: &mut Vec<String>, name: &str, enabled: bool) {
    if enabled {
        list.retain(|n| n != name);
    } else if !list.iter().any(|n| n == name) {
        list.push(name.to_string());
    }
}

pub async fn list_installed(
    host: &dyn Host,
    instance: &AppInstance,
) -> Result<Vec<KarinPluginInstalled>, AppFrameworkError> {
    let root = HostPath::from_posix(&instance.install_dir);
    let disabled = disabled_names(host, &root).await;
    let mut out = Vec::new();
    scan_npm(host, &root, &disabled, &mut out).await?;
    scan_declared_npm(host, &root, &disabled, &mut out).await?;
    scan_git(host, &root, &disabled, &mut out).await?;
    scan_app(host, &root, &disabled, &mut out).await?;
    Ok(out)
}

pub async fn pnpm_add(
    host: &dyn Host,
    install_dir: &HostPath,
    name: &str,
    allow_build: &[String],
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    reject_unsafe_name(name)?;
    let workspace = has_workspace(host, install_dir).await?;
    let mut args = vec!["add", name, "--save"];
    if workspace {
        args.push("-w");
    }
    emit_log(log, format!("pnpm {}", args.join(" ")));
    run_pnpm(host, install_dir, install_dir, &args, log).await?;
    merge_only_built_dependencies(host, install_dir, allow_build).await?;
    if !allow_build.is_empty() {
        let mut install = vec!["install"];
        if workspace {
            install.push("-w");
        }
        emit_log(log, "pnpm install（allowBuild）");
        run_pnpm(host, install_dir, install_dir, &install, log).await?;
    }
    Ok(())
}

pub async fn pnpm_remove(
    host: &dyn Host,
    install_dir: &HostPath,
    name: &str,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    reject_unsafe_name(name)?;
    let workspace = has_workspace(host, install_dir).await?;
    let mut args = vec!["remove", name];
    if workspace {
        args.push("-w");
    }
    emit_log(log, format!("pnpm {}", args.join(" ")));
    run_pnpm(host, install_dir, install_dir, &args, log).await
}

pub async fn git_clone(
    host: &dyn Host,
    install_dir: &HostPath,
    name: &str,
    url: &str,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    reject_unsafe_name(name)?;
    if !host.command_exists("git").await {
        return Err(AppFrameworkError::Validation("主机未安装 git".into()));
    }
    let dest = install_dir.join("plugins").join(name);
    if host.exists(&dest).await.map_err(host_err)? {
        return Err(AppFrameworkError::Validation(format!("已存在: {name}")));
    }
    host.create_dir_all(&install_dir.join("plugins"))
        .await
        .map_err(host_err)?;
    let dest_arg = dest.render_for(host.os());
    emit_log(log, format!("git clone --depth=1 {url}"));
    let cmd = HostCommand::new("git")
        .arg("clone")
        .arg("--depth=1")
        .arg(url)
        .arg(&dest_arg)
        .working_dir(install_dir.clone())
        .timeout(PLUGIN_CMD_TIMEOUT);
    run_host_cmd(host, cmd, "git clone", log).await?;
    maybe_pnpm_install_in(host, install_dir, &dest, log).await
}

pub async fn git_pull(
    host: &dyn Host,
    install_dir: &HostPath,
    name: &str,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    reject_unsafe_name(name)?;
    if !host.command_exists("git").await {
        return Err(AppFrameworkError::Validation("主机未安装 git".into()));
    }
    let dest = install_dir.join("plugins").join(name);
    if !host.exists(&dest).await.map_err(host_err)? {
        return Err(AppFrameworkError::Validation(format!("未安装: {name}")));
    }
    let dest_arg = dest.render_for(host.os());
    emit_log(log, format!("git pull --ff-only {name}"));
    let cmd = HostCommand::new("git")
        .arg("-C")
        .arg(&dest_arg)
        .arg("pull")
        .arg("--ff-only")
        .timeout(PLUGIN_CMD_TIMEOUT);
    run_host_cmd(host, cmd, "git pull", log).await?;
    maybe_pnpm_install_in(host, install_dir, &dest, log).await
}

pub async fn remove_plugin_dir(
    host: &dyn Host,
    install_dir: &HostPath,
    name: &str,
) -> Result<(), AppFrameworkError> {
    reject_unsafe_name(name)?;
    let dest = install_dir.join("plugins").join(name);
    if host.exists(&dest).await.map_err(host_err)? {
        host.remove_dir_all(&dest).await.map_err(host_err)?;
    }
    Ok(())
}

pub async fn write_app_file_bytes(
    host: &dyn Host,
    install_dir: &HostPath,
    basename: &str,
    bytes: &[u8],
) -> Result<(), AppFrameworkError> {
    reject_unsafe_name(basename)?;
    let dest = install_dir.join(APP_PLUGIN_DIR).join(basename);
    if let Some(parent) = dest.parent() {
        host.create_dir_all(&parent).await.map_err(host_err)?;
    }
    host.write_file(&dest, bytes).await.map_err(host_err)
}

pub async fn remove_app_file(
    host: &dyn Host,
    install_dir: &HostPath,
    basename: &str,
) -> Result<(), AppFrameworkError> {
    reject_unsafe_name(basename)?;
    let dest = install_dir.join(APP_PLUGIN_DIR).join(basename);
    if host.exists(&dest).await.map_err(host_err)? {
        host.remove_file(&dest).await.map_err(host_err)?;
    }
    Ok(())
}

pub async fn install_plugin(
    host: &dyn Host,
    instance: &AppInstance,
    entry: &KarinPluginMarketEntry,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    let root = HostPath::from_posix(&instance.install_dir);
    match entry.kind {
        KarinPluginKind::Npm => pnpm_add(host, &root, &entry.name, &entry.allow_build, log).await,
        KarinPluginKind::Git => {
            let url = git_clone_url(entry).ok_or_else(|| {
                AppFrameworkError::Validation(format!("{} 缺少 git 仓库", entry.name))
            })?;
            git_clone(host, &root, &entry.name, url, log).await
        }
        KarinPluginKind::App => Err(AppFrameworkError::Validation(
            "app 型插件由编排层下载".into(),
        )),
    }
}

pub async fn update_plugin(
    host: &dyn Host,
    instance: &AppInstance,
    entry: &KarinPluginMarketEntry,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    let root = HostPath::from_posix(&instance.install_dir);
    match entry.kind {
        KarinPluginKind::Npm => pnpm_add(host, &root, &entry.name, &entry.allow_build, log).await,
        KarinPluginKind::Git => git_pull(host, &root, &entry.name, log).await,
        KarinPluginKind::App => Err(AppFrameworkError::Validation(
            "app 型插件由编排层下载".into(),
        )),
    }
}

pub async fn uninstall_plugin(
    host: &dyn Host,
    instance: &AppInstance,
    name: &str,
    kind: KarinPluginKind,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    let root = HostPath::from_posix(&instance.install_dir);
    match kind {
        KarinPluginKind::Npm => pnpm_remove(host, &root, name, log).await,
        KarinPluginKind::Git => {
            emit_log(log, format!("删除 plugins/{name}"));
            remove_plugin_dir(host, &root, name).await
        }
        KarinPluginKind::App => {
            emit_log(log, format!("删除 {name}"));
            remove_app_file(host, &root, name).await
        }
    }
}

pub async fn confirm_plugin_on_disk(
    host: &dyn Host,
    instance: &AppInstance,
    entry: &KarinPluginMarketEntry,
) -> Result<(), AppFrameworkError> {
    let root = HostPath::from_posix(&instance.install_dir);
    match entry.kind {
        KarinPluginKind::Npm => {
            let pkg = npm_package_dir(&root, &entry.name).join("package.json");
            if host.exists(&pkg).await.map_err(host_err)? {
                return Ok(());
            }
            Err(AppFrameworkError::Runtime(format!(
                "命令已结束，但未找到 {}",
                entry.name
            )))
        }
        KarinPluginKind::Git => {
            let dest = root.join("plugins").join(&entry.name);
            if host.exists(&dest).await.map_err(host_err)? {
                return Ok(());
            }
            Err(AppFrameworkError::Runtime(format!(
                "命令已结束，但未找到 {}",
                entry.name
            )))
        }
        KarinPluginKind::App => {
            for file in &entry.files {
                let basename = app_file_basename(&file.url)?;
                let dest = root.join(APP_PLUGIN_DIR).join(&basename);
                if !host.exists(&dest).await.map_err(host_err)? {
                    return Err(AppFrameworkError::Runtime(format!(
                        "命令已结束，但未找到 {basename}"
                    )));
                }
            }
            Ok(())
        }
    }
}

/// Karin 把 `package.json` 的 `name` 里 `/` 换成 `-`，目录在 `@karinjs/<name>`。
pub fn plugin_data_dir_name(package_name: &str) -> String {
    package_name.replace('/', "-")
}

pub fn plugin_doc_id(plugin_name: &str, rel: &str) -> String {
    format!("plugin:{plugin_name}:{rel}")
}

pub fn parse_plugin_doc_id(doc_id: &str) -> Option<(&str, &str)> {
    let rest = doc_id.strip_prefix("plugin:")?;
    rest.split_once(':')
}

pub fn plugin_config_document(
    plugin_name: &str,
    rel: &str,
) -> Result<AppConfigDocument, AppFrameworkError> {
    reject_plugin_rel(plugin_name, rel)?;
    let dir = plugin_data_dir_name(plugin_name);
    Ok(AppConfigDocument {
        id: plugin_doc_id(plugin_name, rel),
        label: rel.rsplit('/').next().unwrap_or(rel).to_string(),
        rel_path: format!("@karinjs/{dir}/{rel}"),
        format: format_from_rel(rel),
        hot_reload: true,
    })
}

pub fn resolve_plugin_config_doc(doc_id: &str) -> Option<AppConfigDocument> {
    let (name, rel) = parse_plugin_doc_id(doc_id)?;
    plugin_config_document(name, rel).ok()
}

pub async fn list_plugin_config_docs(
    host: &dyn Host,
    instance: &AppInstance,
    plugin_name: &str,
) -> Result<Vec<AppConfigDocument>, AppFrameworkError> {
    reject_unsafe_name(plugin_name)?;
    let root = HostPath::from_posix(&instance.install_dir);
    let mut rels = Vec::new();
    let user_root = root
        .join("@karinjs")
        .join(plugin_data_dir_name(plugin_name));
    let user_config = user_root.join("config");
    if host.exists(&user_config).await.map_err(host_err)? {
        collect_config_rels(host, &user_config, "config", 0, &mut rels).await?;
    } else if host.exists(&user_root).await.map_err(host_err)? {
        collect_config_rels(host, &user_root, "", 0, &mut rels).await?;
    }
    for pkg_config in package_config_dirs(&root, plugin_name) {
        if !host.exists(&pkg_config).await.map_err(host_err)? {
            continue;
        }
        collect_config_rels(host, &pkg_config, "config", 0, &mut rels).await?;
    }
    rels.sort();
    rels.dedup();
    rels.into_iter()
        .map(|rel| plugin_config_document(plugin_name, &rel))
        .collect()
}

pub async fn read_plugin_package_default(
    host: &dyn Host,
    instance: &AppInstance,
    plugin_name: &str,
    rel: &str,
) -> Result<Option<String>, AppFrameworkError> {
    let Some(under_config) = rel.strip_prefix("config/") else {
        return Ok(None);
    };
    let root = HostPath::from_posix(&instance.install_dir);
    for dir in package_config_dirs(&root, plugin_name) {
        let path = dir.join(under_config);
        if !host.exists(&path).await.map_err(host_err)? {
            continue;
        }
        let bytes = host.read_file(&path).await.map_err(host_err)?;
        return Ok(Some(String::from_utf8_lossy(&bytes).into_owned()));
    }
    Ok(None)
}

fn package_config_dirs(root: &HostPath, plugin_name: &str) -> Vec<HostPath> {
    vec![
        npm_package_dir(root, plugin_name).join("config"),
        root.join("plugins").join(plugin_name).join("config"),
    ]
}

fn format_from_rel(rel: &str) -> AppConfigFormat {
    let lower = rel.to_ascii_lowercase();
    if lower.ends_with(".toml") {
        AppConfigFormat::Toml
    } else if lower.ends_with(".json") {
        AppConfigFormat::Json
    } else {
        AppConfigFormat::DotEnv
    }
}

fn is_config_file_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".json") || lower.ends_with(".toml") || lower.ends_with(".yaml") || lower.ends_with(".yml")
}

fn reject_plugin_rel(plugin_name: &str, rel: &str) -> Result<(), AppFrameworkError> {
    reject_unsafe_name(plugin_name)?;
    if rel.is_empty() || rel.contains("..") || rel.contains('\\') || rel.starts_with('/') {
        return Err(AppFrameworkError::Validation("插件配置路径非法".into()));
    }
    if !is_config_file_name(rel) {
        return Err(AppFrameworkError::Validation("只支持 JSON / TOML / YAML".into()));
    }
    Ok(())
}

async fn collect_config_rels(
    host: &dyn Host,
    dir: &HostPath,
    prefix: &str,
    depth: u8,
    out: &mut Vec<String>,
) -> Result<(), AppFrameworkError> {
    if depth > 3 {
        return Ok(());
    }
    for entry in list_dir_or_empty(host, dir).await? {
        if entry.name.starts_with('.') {
            continue;
        }
        let rel = if prefix.is_empty() {
            entry.name.clone()
        } else {
            format!("{prefix}/{}", entry.name)
        };
        if entry.is_dir {
            if matches!(
                entry.name.as_str(),
                "data" | "temp" | "node_modules" | "dist" | "src"
            ) {
                continue;
            }
            Box::pin(collect_config_rels(
                host,
                &dir.join(&entry.name),
                &rel,
                depth + 1,
                out,
            ))
            .await?;
        } else if is_config_file_name(&entry.name) && !out.iter().any(|x| x == &rel) {
            out.push(rel);
        }
    }
    Ok(())
}

async fn scan_npm(
    host: &dyn Host,
    root: &HostPath,
    disabled: &HashSet<String>,
    out: &mut Vec<KarinPluginInstalled>,
) -> Result<(), AppFrameworkError> {
    let nm = root.join("node_modules");
    if !host.exists(&nm).await.map_err(host_err)? {
        return Ok(());
    }
    for entry in list_dir_or_empty(host, &nm).await? {
        if entry.name.starts_with('.') {
            continue;
        }
        if entry.name.starts_with('@') {
            let scope = nm.join(&entry.name);
            for inner in list_dir_or_empty(host, &scope).await? {
                if inner.name.starts_with('.') {
                    continue;
                }
                maybe_push_npm(
                    host,
                    &scope.join(&inner.name),
                    &format!("{}/{}", entry.name, inner.name),
                    disabled,
                    out,
                )
                .await?;
            }
        } else {
            maybe_push_npm(host, &nm.join(&entry.name), &entry.name, disabled, out).await?;
        }
    }
    Ok(())
}

async fn scan_declared_npm(
    host: &dyn Host,
    root: &HostPath,
    disabled: &HashSet<String>,
    out: &mut Vec<KarinPluginInstalled>,
) -> Result<(), AppFrameworkError> {
    for name in read_package_dep_names(host, &root.join("package.json")).await? {
        if out.iter().any(|p| p.name == name) {
            continue;
        }
        maybe_push_npm(host, &npm_package_dir(root, &name), &name, disabled, out).await?;
    }
    Ok(())
}

async fn maybe_push_npm(
    host: &dyn Host,
    pkg_dir: &HostPath,
    fallback_name: &str,
    disabled: &HashSet<String>,
    out: &mut Vec<KarinPluginInstalled>,
) -> Result<(), AppFrameworkError> {
    let Some((name, version)) = read_karin_package(host, pkg_dir, fallback_name).await? else {
        return Ok(());
    };
    let name = if name.is_empty() {
        fallback_name.to_string()
    } else {
        name
    };
    let enabled = !disabled.contains(&name);
    out.push(KarinPluginInstalled {
        name,
        kind: KarinPluginKind::Npm,
        version,
        enabled,
    });
    Ok(())
}

async fn scan_git(
    host: &dyn Host,
    root: &HostPath,
    disabled: &HashSet<String>,
    out: &mut Vec<KarinPluginInstalled>,
) -> Result<(), AppFrameworkError> {
    let plugins = root.join("plugins");
    if !host.exists(&plugins).await.map_err(host_err)? {
        return Ok(());
    }
    for entry in list_dir_or_empty(host, &plugins).await? {
        if !entry.is_dir || !entry.name.starts_with("karin-plugin-") {
            continue;
        }
        if entry.name == "karin-plugin-example" {
            continue;
        }
        let dir = plugins.join(&entry.name);
        let version = match read_package_version(host, &dir).await {
            Ok(v) => v,
            Err(_) => None,
        };
        let enabled = !disabled.contains(&entry.name);
        out.push(KarinPluginInstalled {
            name: entry.name,
            kind: KarinPluginKind::Git,
            version,
            enabled,
        });
    }
    Ok(())
}

async fn scan_app(
    host: &dyn Host,
    root: &HostPath,
    disabled: &HashSet<String>,
    out: &mut Vec<KarinPluginInstalled>,
) -> Result<(), AppFrameworkError> {
    let dir = root.join(APP_PLUGIN_DIR);
    if !host.exists(&dir).await.map_err(host_err)? {
        return Ok(());
    }
    for entry in list_dir_or_empty(host, &dir).await? {
        if entry.is_dir {
            continue;
        }
        let lower = entry.name.to_ascii_lowercase();
        if !lower.ends_with(".js") && !lower.ends_with(".ts") {
            continue;
        }
        let enabled = !disabled.contains(&entry.name);
        out.push(KarinPluginInstalled {
            name: entry.name,
            kind: KarinPluginKind::App,
            version: None,
            enabled,
        });
    }
    Ok(())
}

async fn disabled_names(host: &dyn Host, root: &HostPath) -> HashSet<String> {
    let mut out = HashSet::new();
    for file in ["groups.json", "privates.json"] {
        let path = root.join(KARIN_CONFIG_DIR).join(file);
        let Ok(bytes) = host.read_file(&path).await else {
            continue;
        };
        let Ok(rules) = serde_json::from_slice::<Vec<KarinScopeRule>>(&bytes) else {
            continue;
        };
        if let Some(rule) = rules.iter().find(|r| r.key == "default") {
            out.extend(rule.disable.iter().cloned());
        }
    }
    out
}

async fn read_karin_package(
    host: &dyn Host,
    pkg_dir: &HostPath,
    fallback_name: &str,
) -> Result<Option<(String, Option<String>)>, AppFrameworkError> {
    let path = pkg_dir.join("package.json");
    if !host.exists(&path).await.map_err(host_err)? {
        return Ok(None);
    }
    let bytes = match host.read_file(&path).await {
        Ok(b) => b,
        Err(_) => return Ok(None),
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return Ok(None);
    };
    let name = value
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let probe = if name.is_empty() {
        fallback_name
    } else {
        name.as_str()
    };
    if !is_likely_karin_npm(probe, &value) {
        return Ok(None);
    }
    let version = value
        .get("version")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    Ok(Some((name, version)))
}

async fn read_package_dep_names(
    host: &dyn Host,
    path: &HostPath,
) -> Result<Vec<String>, AppFrameworkError> {
    if !host.exists(path).await.map_err(host_err)? {
        return Ok(Vec::new());
    }
    let bytes = match host.read_file(path).await {
        Ok(b) => b,
        Err(_) => return Ok(Vec::new()),
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return Ok(Vec::new());
    };
    let mut names = Vec::new();
    for key in ["dependencies", "optionalDependencies", "devDependencies"] {
        let Some(obj) = value.get(key).and_then(|v| v.as_object()) else {
            continue;
        };
        for name in obj.keys() {
            if !names.iter().any(|n| n == name) {
                names.push(name.clone());
            }
        }
    }
    Ok(names)
}

async fn read_package_version(
    host: &dyn Host,
    dir: &HostPath,
) -> Result<Option<String>, AppFrameworkError> {
    let path = dir.join("package.json");
    if !host.exists(&path).await.map_err(host_err)? {
        return Ok(None);
    }
    let bytes = host.read_file(&path).await.map_err(host_err)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| AppFrameworkError::Validation(format!("package.json 无效: {e}")))?;
    Ok(value
        .get("version")
        .and_then(|v| v.as_str())
        .map(str::to_string))
}

async fn merge_only_built_dependencies(
    host: &dyn Host,
    install_dir: &HostPath,
    names: &[String],
) -> Result<(), AppFrameworkError> {
    if names.is_empty() {
        return Ok(());
    }
    let path = install_dir.join("package.json");
    if !host.exists(&path).await.map_err(host_err)? {
        return Ok(());
    }
    let bytes = host.read_file(&path).await.map_err(host_err)?;
    let mut value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| AppFrameworkError::Validation(format!("package.json 无效: {e}")))?;
    let obj = value
        .as_object_mut()
        .ok_or_else(|| AppFrameworkError::Validation("package.json 须为对象".into()))?;
    let pnpm = obj
        .entry("pnpm")
        .or_insert_with(|| serde_json::json!({}));
    let pnpm_obj = pnpm
        .as_object_mut()
        .ok_or_else(|| AppFrameworkError::Validation("package.json pnpm 须为对象".into()))?;
    let arr = pnpm_obj
        .entry("onlyBuiltDependencies")
        .or_insert_with(|| serde_json::json!([]));
    let list = arr.as_array_mut().ok_or_else(|| {
        AppFrameworkError::Validation("pnpm.onlyBuiltDependencies 须为数组".into())
    })?;
    for name in names {
        if !list.iter().any(|v| v.as_str() == Some(name)) {
            list.push(serde_json::Value::String(name.clone()));
        }
    }
    let pretty = serde_json::to_vec_pretty(&value)
        .map_err(|e| AppFrameworkError::Validation(format!("序列化 package.json 失败: {e}")))?;
    host.write_file(&path, &pretty).await.map_err(host_err)
}

async fn maybe_pnpm_install_in(
    host: &dyn Host,
    instance_root: &HostPath,
    plugin_dir: &HostPath,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    let pkg = plugin_dir.join("package.json");
    if !host.exists(&pkg).await.map_err(host_err)? {
        return Ok(());
    }
    run_pnpm(host, instance_root, plugin_dir, &["install"], log).await
}

async fn run_pnpm(
    host: &dyn Host,
    instance_root: &HostPath,
    cwd: &HostPath,
    args: &[&str],
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    let mut preferred = Vec::new();
    if let Some(marker) = read_node_marker(host, instance_root).await {
        preferred.push(marker);
    }
    let tc = resolve_node_toolchain(host, &preferred)
        .await
        .map_err(|e| AppFrameworkError::Runtime(e.to_string()))?;
    let mut cmd = pnpm_command(&tc, instance_root, host.os(), args)
        .working_dir(cwd.clone())
        .timeout(PLUGIN_CMD_TIMEOUT)
        .env("CI", "true")
        .env("npm_config_update_notifier", "false");
    if host.locality() == Locality::Local {
        let prefix = path_prefix(&tc, instance_root, host.os());
        cmd = cmd.env("PATH", local_path_env(&prefix, host.os()));
    }
    run_host_cmd(
        host,
        cmd,
        &format!("pnpm {}", args.first().unwrap_or(&"")),
        log,
    )
    .await
}

async fn run_host_cmd(
    host: &dyn Host,
    cmd: HostCommand,
    label: &str,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    let out = if let Some(sink) = log {
        let sink = sink.clone();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let drain = tokio::spawn(async move {
            while let Some(line) = rx.recv().await {
                sink(line);
            }
        });
        let result = host
            .run_streaming(
                cmd,
                Box::new(move |_src, line| {
                    let trimmed = line.trim_end();
                    if !trimmed.is_empty() {
                        let _ = tx.send(trimmed.to_string());
                    }
                }),
            )
            .await;
        let _ = drain.await;
        result.map_err(host_err)?
    } else {
        host.run_to_string(cmd).await.map_err(host_err)?
    };
    if !out.success() {
        let detail = out
            .stderr
            .trim()
            .lines()
            .last()
            .or_else(|| out.stdout.trim().lines().last())
            .unwrap_or_default();
        return Err(AppFrameworkError::Runtime(format!(
            "{label}: exit={:?}: {detail}",
            out.exit_code
        )));
    }
    Ok(())
}

fn emit_log(log: Option<&PluginLogSink>, message: impl Into<String>) {
    if let Some(sink) = log {
        sink(message.into());
    }
}

fn npm_package_dir(root: &HostPath, name: &str) -> HostPath {
    root.join("node_modules").join(name)
}

fn is_likely_karin_npm(name: &str, value: &serde_json::Value) -> bool {
    value.get("karin").is_some()
        || name.starts_with("@karinjs/plugin-")
        || name.starts_with("karin-plugin-")
}

async fn has_workspace(host: &dyn Host, install_dir: &HostPath) -> Result<bool, AppFrameworkError> {
    host.exists(&install_dir.join("pnpm-workspace.yaml"))
        .await
        .map_err(host_err)
}

async fn list_dir_or_empty(
    host: &dyn Host,
    path: &HostPath,
) -> Result<Vec<ncd_host::DirEntry>, AppFrameworkError> {
    match host.list_dir(path).await {
        Ok(v) => Ok(v),
        Err(HostError::PathNotFound { .. }) => Ok(Vec::new()),
        Err(e) => Err(host_err(e)),
    }
}

fn reject_unsafe_name(name: &str) -> Result<(), AppFrameworkError> {
    if name.is_empty() || name.contains("..") || name.contains('\\') {
        return Err(AppFrameworkError::Validation("插件名非法".into()));
    }
    Ok(())
}

fn host_err(e: HostError) -> AppFrameworkError {
    AppFrameworkError::Host(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_domain::{AppFrameworkId, AppInstanceId, AppInstanceState, AppPlacement};

    #[test]
    fn parse_official_plugins_list_sample() {
        let json = r#"{
          "name":"@karinjs/plugins-list",
          "plugins":[
            {
              "name":"@karinjs/plugin-basic",
              "type":"npm",
              "description":"karin plugin basic",
              "time":"2025-01-19 10:00:00",
              "home":"https://github.com/karinjs/karin-plugin-basic",
              "author":[{"home":"https://github.com/sj817","name":"shijin"}],
              "repo":[{"url":"https://github.com/karinjs/karin-plugin-basic","type":"github","branch":"main"}]
            },
            {
              "name":"karin-plugins-alijs",
              "type":"app",
              "description":"插件集合",
              "time":"2025-02-16 11:45:14",
              "home":"https://github.com/Aliorpse/karin-plugins-alijs",
              "author":[{"home":"https://github.com/Aliorpse","name":"Aliorpse"}],
              "repo":[{"url":"https://github.com/Aliorpse/karin-plugins-alijs","type":"github","branch":"main"}],
              "files":[{
                "url":"https://raw.githubusercontent.com/Aliorpse/karin-plugins-alijs/refs/heads/main/js/BiliParser.js",
                "name":"B站链接解析",
                "description":"解析B站链接"
              }],
              "allowBuild":["foo"]
            }
          ]
        }"#;
        let list = parse_karin_plugins_list(json).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].kind, KarinPluginKind::Npm);
        assert_eq!(list[0].authors[0].name, "shijin");
        assert_eq!(
            git_clone_url(&list[0]),
            Some("https://github.com/karinjs/karin-plugin-basic")
        );
        assert_eq!(list[1].kind, KarinPluginKind::App);
        assert!(list[1].files[0].url.contains("BiliParser.js"));
        assert_eq!(list[1].allow_build, vec!["foo".to_string()]);
    }

    #[test]
    fn app_file_url_must_be_js_or_ts() {
        assert!(app_file_basename("https://x.com/a.js").is_ok());
        assert!(app_file_basename("https://x.com/a.ts?raw=1").is_ok());
        assert!(app_file_basename("https://x.com/a.exe").is_err());
    }

    #[test]
    fn plugin_config_doc_id_round_trips() {
        assert_eq!(plugin_data_dir_name("@karinjs/plugin-basic"), "@karinjs-plugin-basic");
        let id = plugin_doc_id("@karinjs/plugin-basic", "config/config.json");
        assert_eq!(
            parse_plugin_doc_id(&id),
            Some(("@karinjs/plugin-basic", "config/config.json"))
        );
        let doc = plugin_config_document("@karinjs/plugin-basic", "config/config.json").unwrap();
        assert_eq!(doc.rel_path, "@karinjs/@karinjs-plugin-basic/config/config.json");
        assert!(doc.hot_reload);
        assert!(plugin_config_document("@karinjs/plugin-basic", "../x.json").is_err());
    }

    #[test]
    fn apply_plugin_enabled_only_touches_default_disable() {
        let mut cfg = KarinInstanceConfig::default();
        cfg.groups = vec![
            KarinScopeRule {
                key: "default".into(),
                disable: vec!["keep-me".into()],
                ..KarinScopeRule::group("default")
            },
            KarinScopeRule {
                key: "Bot:1".into(),
                disable: vec!["other".into()],
                ..KarinScopeRule::group("Bot:1")
            },
        ];
        cfg.privates = vec![KarinScopeRule::private("default")];

        apply_plugin_enabled(&mut cfg, "@karinjs/plugin-basic", false).unwrap();
        assert_eq!(
            cfg.groups[0].disable,
            vec!["keep-me".to_string(), "@karinjs/plugin-basic".to_string()]
        );
        assert_eq!(cfg.groups[1].disable, vec!["other".to_string()]);
        assert_eq!(
            cfg.privates[0].disable,
            vec!["@karinjs/plugin-basic".to_string()]
        );

        apply_plugin_enabled(&mut cfg, "@karinjs/plugin-basic", true).unwrap();
        assert_eq!(cfg.groups[0].disable, vec!["keep-me".to_string()]);
        assert!(cfg.privates[0].disable.is_empty());
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    fn sample_instance(dir: HostPath) -> AppInstance {
        AppInstance {
            id: AppInstanceId::new("k1"),
            framework_id: AppFrameworkId::new("karin"),
            display_name: "K".into(),
            placement: AppPlacement::LocalNative,
            host_id: "local".into(),
            install_dir: dir.as_posix().to_string(),
            port: 7777,
            state: AppInstanceState::Installed,
            link: None,
            installed_version: None,
            last_error: None,
            created_at_ms: 1,
            install_renderer: true,
        }
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn list_installed_classifies_npm_git_app() {
        use crate::KarinAdapter;
        use crate::adapter::AppFrameworkAdapter;

        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("node_modules/@karinjs/plugin-basic")).unwrap();
        std::fs::write(
            root.join("node_modules/@karinjs/plugin-basic/package.json"),
            r#"{"name":"@karinjs/plugin-basic","version":"1.2.3","karin":{}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(root.join("plugins/karin-plugin-foo")).unwrap();
        std::fs::write(
            root.join("plugins/karin-plugin-foo/package.json"),
            r#"{"name":"karin-plugin-foo","version":"0.1.0"}"#,
        )
        .unwrap();
        std::fs::create_dir_all(root.join("plugins/karin-plugin-example")).unwrap();
        std::fs::write(
            root.join("plugins/karin-plugin-example/hello.js"),
            b"export {}",
        )
        .unwrap();

        let host = ncd_host::local::LocalWindowsHost::new();
        let inst = sample_instance(HostPath::from_windows(root.to_str().unwrap()));
        let list = KarinAdapter::new()
            .list_installed(&host, &inst, ncd_domain::AppStoreResource::Plugin)
            .await
            .unwrap();
        let names: Vec<_> = list.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"@karinjs/plugin-basic"));
        assert!(names.contains(&"karin-plugin-foo"));
        assert!(names.contains(&"hello.js"));
        assert_eq!(
            list.iter()
                .find(|p| p.name == "@karinjs/plugin-basic")
                .unwrap()
                .version
                .as_deref(),
            Some("1.2.3")
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn list_installed_picks_declared_npm_without_karin_key() {
        use crate::KarinAdapter;
        use crate::adapter::AppFrameworkAdapter;

        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("node_modules/@karinjs/plugin-basic")).unwrap();
        std::fs::write(
            root.join("node_modules/@karinjs/plugin-basic/package.json"),
            r#"{"name":"@karinjs/plugin-basic","version":"1.4.13"}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("package.json"),
            r#"{"dependencies":{"@karinjs/plugin-basic":"1.4.13"}}"#,
        )
        .unwrap();

        let host = ncd_host::local::LocalWindowsHost::new();
        let inst = sample_instance(HostPath::from_windows(root.to_str().unwrap()));
        let list = KarinAdapter::new()
            .list_installed(&host, &inst, ncd_domain::AppStoreResource::Plugin)
            .await
            .unwrap();
        assert!(list.iter().any(|p| p.name == "@karinjs/plugin-basic"));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn list_plugin_config_docs_from_user_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("@karinjs/@karinjs-plugin-basic/config")).unwrap();
        std::fs::write(
            root.join("@karinjs/@karinjs-plugin-basic/config/config.json"),
            b"{\"ok\":true}",
        )
        .unwrap();
        let host = ncd_host::local::LocalWindowsHost::new();
        let inst = sample_instance(HostPath::from_windows(root.to_str().unwrap()));
        let docs = list_plugin_config_docs(&host, &inst, "@karinjs/plugin-basic")
            .await
            .unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].rel_path, "@karinjs/@karinjs-plugin-basic/config/config.json");
        assert!(docs[0].hot_reload);
    }
}
