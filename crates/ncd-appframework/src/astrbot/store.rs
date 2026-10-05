//! 只代管官方插件市场。不实现 Adapter resource。

use std::time::Duration;

use ncd_domain::{AppInstance, AppStoreResource};
use ncd_host::{ArchiveKind, Host, HostCommand, HostError, HostPath};
use ncd_traits::AppFrameworkError;
use serde_json::{Map, Value};

use super::manifest::{ASTRBOT_PLUGINS_DIR, ASTRBOT_SHARED_PREFS};
use crate::adapter::PluginLogSink;
use crate::store::{AppStoreFlavor, AppStoreInstalled, AppStoreMarketEntry};
use crate::uv_tooling::{read_uv_marker, resolve_uv};

const CMD_TIMEOUT: Duration = Duration::from_secs(20 * 60);
pub const ASTRBOT_PLUGINS_URL: &str = "https://api.soulter.top/astrbot/plugins";
const ASTRBOT_PLUGINS_GITHUB: &str = "https://github.com/AstrBotDevs/AstrBot_Plugins_Collection/raw/refs/heads/main/plugin_cache_original.json";
const ASTRBOT_PLUGINS_GHPROXY: &str = "https://gh-proxy.com/https://raw.githubusercontent.com/AstrBotDevs/AstrBot_Plugins_Collection/main/plugin_cache_original.json";

pub fn astrbot_plugin_market_urls() -> Vec<String> {
    vec![
        ASTRBOT_PLUGINS_URL.to_string(),
        ASTRBOT_PLUGINS_GITHUB.to_string(),
        ASTRBOT_PLUGINS_GHPROXY.to_string(),
    ]
}

pub fn parse_astrbot_plugins_json(
    text: &str,
) -> Result<Vec<AppStoreMarketEntry>, AppFrameworkError> {
    let v: Value = serde_json::from_str(text)
        .map_err(|e| AppFrameworkError::Validation(format!("解析 AstrBot 插件目录失败: {e}")))?;
    let obj = v
        .as_object()
        .ok_or_else(|| AppFrameworkError::Validation("AstrBot 插件目录根必须是对象".into()))?;
    let mut out = Vec::new();
    for (key, item) in obj {
        if key.starts_with('$') {
            continue;
        }
        let Some(item) = item.as_object() else {
            continue;
        };
        let author = str_field(item, "author");
        let name = str_field(item, "name");
        if name.is_empty() && author.is_empty() && !key.contains('/') {
            continue;
        }
        let plugin_id = if key.contains('/') {
            key.clone()
        } else if !author.is_empty() && !name.is_empty() {
            format!("{author}/{name}")
        } else {
            key.clone()
        };
        let repo = str_field(item, "repo");
        let download_url = str_field(item, "download_url");
        let desc = first_field(item, &["desc", "description"]);
        let mut files = Vec::new();
        if !download_url.is_empty() {
            files.push(crate::karin::plugin::KarinPluginAppFile {
                url: download_url,
                name: "archive.zip".into(),
                description: String::new(),
            });
        }
        out.push(AppStoreMarketEntry {
            resource: AppStoreResource::Plugin,
            id: plugin_id,
            name: if name.is_empty() { key.clone() } else { name },
            description: desc,
            version: str_field(item, "version"),
            author: author.clone(),
            homepage: repo.clone(),
            time: String::new(),
            package: repo,
            module_name: String::new(),
            flavor: AppStoreFlavor::Git,
            is_official: false,
            valid: true,
            tags: string_list(item, "tags"),
            supported_adapters: string_list(item, "support_platforms"),
            authors: Vec::new(),
            repos: Vec::new(),
            files,
            allow_build: Vec::new(),
        });
    }
    Ok(out)
}

fn str_field(obj: &Map<String, Value>, key: &str) -> String {
    obj.get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

fn first_field(obj: &Map<String, Value>, keys: &[&str]) -> String {
    keys.iter()
        .map(|k| str_field(obj, k))
        .find(|s| !s.is_empty())
        .unwrap_or_default()
}

fn string_list(obj: &Map<String, Value>, key: &str) -> Vec<String> {
    obj.get(key)
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

// 插件作者常在字段后带行内注释（name: foo # 唯一识别名）。按 YAML 规则取标量：
// 引号里的 # 是内容；裸标量里前面带空白的 # 起才是注释。
fn yaml_scalar_value(raw: &str) -> String {
    let s = raw.trim();
    if let Some(rest) = s.strip_prefix('"') {
        let mut out = String::new();
        let mut chars = rest.chars();
        while let Some(c) = chars.next() {
            match c {
                '\\' => {
                    if let Some(n) = chars.next() {
                        out.push(n);
                    }
                }
                '"' => break,
                _ => out.push(c),
            }
        }
        return out;
    }
    if let Some(rest) = s.strip_prefix('\'') {
        // 单引号标量里 '' 表示一个字面单引号
        let mut out = String::new();
        let mut chars = rest.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\'' {
                if chars.peek() == Some(&'\'') {
                    chars.next();
                    out.push('\'');
                } else {
                    break;
                }
            } else {
                out.push(c);
            }
        }
        return out;
    }
    let mut end = s.len();
    for (i, c) in s.char_indices() {
        if c == '#' && (i == 0 || s.as_bytes()[i - 1].is_ascii_whitespace()) {
            end = i;
            break;
        }
    }
    s[..end].trim_end().to_string()
}

pub fn parse_metadata_yaml(text: &str) -> PluginMetadata {
    let mut meta = PluginMetadata::default();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('-') {
            continue;
        }
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let key = k.trim();
        let val = yaml_scalar_value(v);
        match key {
            "name" => meta.name = val,
            "author" => meta.author = val,
            "version" => meta.version = val,
            "desc" | "description" => meta.desc = val,
            "repo" => meta.repo = val,
            _ => {}
        }
    }
    meta
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginMetadata {
    pub name: String,
    pub author: String,
    pub version: String,
    pub desc: String,
    pub repo: String,
}

impl PluginMetadata {
    pub fn plugin_id(&self) -> String {
        if self.author.is_empty() {
            self.name.clone()
        } else {
            format!("{}/{}", self.author, self.name)
        }
    }
}

fn reject_unsafe_name(name: &str) -> Result<(), AppFrameworkError> {
    let t = name.trim();
    if t.is_empty() || t.contains("..") || t.contains('/') || t.contains('\\') || t == "." {
        return Err(AppFrameworkError::Validation(format!(
            "非法插件目录名: {name}"
        )));
    }
    Ok(())
}

/// 目录里的一项能不能原名挪到上一层:空名、`.` / `..`、带路径分隔符的挪不了
fn is_movable_child_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\'])
}

fn plugin_dir_name(entry: &AppStoreMarketEntry) -> Result<String, AppFrameworkError> {
    let name = entry
        .id
        .rsplit('/')
        .next()
        .unwrap_or(entry.id.as_str())
        .trim();
    reject_unsafe_name(name)?;
    Ok(name.to_string())
}

pub(super) fn plugins_root(instance: &AppInstance) -> HostPath {
    HostPath::from_posix(&instance.install_dir).join(ASTRBOT_PLUGINS_DIR)
}

fn prefs_path(instance: &AppInstance) -> HostPath {
    HostPath::from_posix(&instance.install_dir).join(ASTRBOT_SHARED_PREFS)
}

fn db_path(instance: &AppInstance) -> HostPath {
    HostPath::from_posix(&instance.install_dir).join("data/data_v4.db")
}

pub fn module_path_for(dir: &str, has_main_py: bool) -> String {
    if has_main_py {
        format!("data.plugins.{dir}.main")
    } else {
        format!("data.plugins.{dir}.{dir}")
    }
}

fn host_err(e: HostError) -> AppFrameworkError {
    AppFrameworkError::Host(e.to_string())
}

pub(super) async fn read_text(
    host: &dyn Host,
    path: &HostPath,
) -> Result<Option<String>, AppFrameworkError> {
    if !host.exists(path).await.map_err(host_err)? {
        return Ok(None);
    }
    let bytes = host.read_file(path).await.map_err(host_err)?;
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

async fn load_prefs(host: &dyn Host, instance: &AppInstance) -> Result<Value, AppFrameworkError> {
    match read_text(host, &prefs_path(instance)).await? {
        Some(text) if !text.trim().is_empty() => serde_json::from_str(&text).map_err(|e| {
            AppFrameworkError::Integration(format!("shared_preferences.json 无法解析: {e}"))
        }),
        _ => Ok(Value::Object(Map::new())),
    }
}

fn inactivated_from_prefs(root: &Value) -> Vec<String> {
    root.get("inactivated_plugins")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

async fn save_prefs(
    host: &dyn Host,
    instance: &AppInstance,
    root: &Value,
) -> Result<(), AppFrameworkError> {
    let path = prefs_path(instance);
    if let Some(parent) = path.parent()
        && !host.exists(&parent).await.map_err(host_err)?
    {
        host.create_dir_all(&parent).await.map_err(host_err)?;
    }
    let text = crate::config_doc::render_json_pretty(root)?;
    host.write_file(&path, text.as_bytes())
        .await
        .map_err(host_err)?;
    sync_prefs_db(host, instance, root).await;
    Ok(())
}

/// v4 起 inactivated_plugins 在 SQLite；JSON 仍写，库存在则尽量同步。
async fn sync_prefs_db(host: &dyn Host, instance: &AppInstance, root: &Value) {
    let db = db_path(instance);
    if !host.exists(&db).await.unwrap_or(false) {
        return;
    }
    if !host.command_exists("sqlite3").await {
        return;
    }
    let list = inactivated_from_prefs(root);
    let Ok(payload) = serde_json::to_string(&serde_json::json!({ "val": list })) else {
        return;
    };
    let escaped = payload.replace('\'', "''");
    let sql = format!(
        "INSERT INTO preferences(scope, scope_id, key, value) VALUES('global','global','inactivated_plugins','{escaped}') \
         ON CONFLICT(scope, scope_id, key) DO UPDATE SET value=excluded.value;"
    );
    let cmd = HostCommand::new("sqlite3")
        .arg(db.render_for(host.os()))
        .arg(sql)
        .timeout(Duration::from_secs(15));
    let _ = host.run_to_string(cmd).await;
}

async fn read_inactivated(
    host: &dyn Host,
    instance: &AppInstance,
) -> Result<Vec<String>, AppFrameworkError> {
    Ok(inactivated_from_prefs(&load_prefs(host, instance).await?))
}

pub async fn list_installed(
    host: &dyn Host,
    instance: &AppInstance,
    resource: AppStoreResource,
) -> Result<Vec<AppStoreInstalled>, AppFrameworkError> {
    if resource != AppStoreResource::Plugin {
        return Ok(Vec::new());
    }
    let root = plugins_root(instance);
    if !host.exists(&root).await.map_err(host_err)? {
        return Ok(Vec::new());
    }
    let inactivated = read_inactivated(host, instance).await?;
    let mut out = Vec::new();
    for entry in host.list_dir(&root).await.map_err(host_err)? {
        if !entry.is_dir || entry.name.starts_with('.') {
            continue;
        }
        if reject_unsafe_name(&entry.name).is_err() {
            continue;
        }
        let dir = root.join(&entry.name);
        let meta_text = read_text(host, &dir.join("metadata.yaml")).await?;
        let meta = meta_text
            .as_deref()
            .map(parse_metadata_yaml)
            .unwrap_or_default();
        let has_main = host.exists(&dir.join("main.py")).await.map_err(host_err)?;
        let module = module_path_for(&entry.name, has_main);
        let id = if meta.author.is_empty() && meta.name.is_empty() {
            entry.name.clone()
        } else {
            meta.plugin_id()
        };
        out.push(AppStoreInstalled {
            id,
            name: if meta.name.is_empty() {
                entry.name.clone()
            } else {
                meta.name
            },
            resource: AppStoreResource::Plugin,
            flavor: AppStoreFlavor::Git,
            version: if meta.version.is_empty() {
                None
            } else {
                Some(meta.version)
            },
            enabled: !inactivated.iter().any(|p| p == &module),
            package: meta.repo,
            locked: false,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn emit_log(log: Option<&PluginLogSink>, line: impl Into<String>) {
    if let Some(sink) = log {
        sink(line.into());
    }
}

async fn run_host_cmd(
    host: &dyn Host,
    cmd: HostCommand,
    name: &str,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    let out = host
        .run_to_string(cmd.timeout(CMD_TIMEOUT))
        .await
        .map_err(host_err)?;
    if !out.stdout.trim().is_empty() {
        emit_log(log, out.stdout.trim());
    }
    if !out.success() {
        return Err(AppFrameworkError::Runtime(format!(
            "{name} 失败: {}",
            out.stderr.trim().lines().last().unwrap_or_default()
        )));
    }
    Ok(())
}

fn github_clone_candidates(repo: &str) -> Result<Vec<String>, AppFrameworkError> {
    let repo = repo.trim().trim_end_matches('/').trim_end_matches(".git");
    if !repo.starts_with("https://github.com/") {
        return Err(AppFrameworkError::Validation(
            "只接受 HTTPS GitHub 仓库".into(),
        ));
    }
    Ok(vec![
        format!("{repo}.git"),
        format!("https://gh-proxy.com/{repo}.git"),
        format!("https://mirror.ghproxy.com/{repo}.git"),
    ])
}

fn github_download_candidates(url: &str) -> Vec<String> {
    let url = url.trim();
    if url.is_empty() {
        return Vec::new();
    }
    let mut out = vec![url.to_string()];
    if url.contains("github.com") || url.contains("githubusercontent.com") {
        out.push(format!("https://gh-proxy.com/{url}"));
        out.push(format!("https://mirror.ghproxy.com/{url}"));
    }
    out
}

/// GitHub 的地址依次试原站和两个代理；MaiBot 的插件商店也用
pub(crate) async fn download_file_with_mirrors(
    host: &dyn Host,
    url: &str,
    dest: &HostPath,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    let mut last = AppFrameworkError::Host("没有可用下载地址".into());
    for candidate in github_download_candidates(url) {
        match download_file(host, &candidate, dest, log).await {
            Ok(()) => return Ok(()),
            Err(e) => last = e,
        }
    }
    Err(last)
}

async fn download_file(
    host: &dyn Host,
    url: &str,
    dest: &HostPath,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    emit_log(log, format!("下载 {url}"));
    match host.download_url(url, dest).await {
        Ok(()) => return Ok(()),
        Err(HostError::Unsupported { .. }) => {}
        Err(e) => return Err(host_err(e)),
    }
    if host.command_exists("curl").await {
        let cmd = HostCommand::new("curl")
            .arg("-L")
            .arg("--fail")
            .arg("-o")
            .arg(dest.render_for(host.os()))
            .arg(url);
        return run_host_cmd(host, cmd, "curl", log).await;
    }
    if host.command_exists("wget").await {
        let cmd = HostCommand::new("wget")
            .arg("-O")
            .arg(dest.render_for(host.os()))
            .arg(url);
        return run_host_cmd(host, cmd, "wget", log).await;
    }
    Err(AppFrameworkError::Host(
        "主机无法下载文件（需要 download_url / curl / wget）".into(),
    ))
}

async fn git_clone_try(
    host: &dyn Host,
    dest: &HostPath,
    urls: &[String],
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    if !host.command_exists("git").await {
        return Err(AppFrameworkError::Validation("主机未安装 git".into()));
    }
    let mut last = String::new();
    for url in urls {
        emit_log(log, format!("git clone --depth=1 {url}"));
        let cmd = HostCommand::new("git")
            .arg("clone")
            .arg("--depth=1")
            .arg(url)
            .arg(dest.render_for(host.os()));
        match run_host_cmd(host, cmd, "git clone", log).await {
            Ok(()) => return Ok(()),
            Err(e) => last = e.to_string(),
        }
        if host.exists(dest).await.unwrap_or(false) {
            let _ = host.remove_dir_all(dest).await;
        }
    }
    Err(AppFrameworkError::Runtime(format!(
        "git clone 失败: {last}"
    )))
}

async fn confirm_metadata(
    host: &dyn Host,
    dest: &HostPath,
    entry: &AppStoreMarketEntry,
) -> Result<PluginMetadata, AppFrameworkError> {
    let text = read_text(host, &dest.join("metadata.yaml"))
        .await?
        .ok_or_else(|| AppFrameworkError::Validation("装完没有 metadata.yaml".into()))?;
    let meta = parse_metadata_yaml(&text);
    if meta.name.is_empty() {
        return Err(AppFrameworkError::Validation(
            "metadata.yaml 缺少 name".into(),
        ));
    }
    let expect = entry.id.to_ascii_lowercase();
    let got = meta.plugin_id().to_ascii_lowercase();
    if !expect.is_empty()
        && expect != got
        && !expect.ends_with(&format!("/{}", meta.name.to_ascii_lowercase()))
    {
        return Err(AppFrameworkError::Validation(format!(
            "插件身份不符：目录是 {}，市场是 {}",
            meta.plugin_id(),
            entry.id
        )));
    }
    Ok(meta)
}

async fn pip_requirements(
    host: &dyn Host,
    instance: &AppInstance,
    dest: &HostPath,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    let req = dest.join("requirements.txt");
    if !host.exists(&req).await.map_err(host_err)? {
        return Ok(());
    }
    let mut preferred = Vec::new();
    if let Some(p) = read_uv_marker(host, &HostPath::from_posix(&instance.install_dir)).await {
        preferred.push(p);
    }
    let uv = resolve_uv(host, &preferred)
        .await
        .map_err(|e| AppFrameworkError::Runtime(e.to_string()))?;
    emit_log(log, "uv pip install -r requirements.txt");
    let cmd = HostCommand::new(uv.uv_bin.as_posix())
        .arg("pip")
        .arg("install")
        .arg("--python")
        .arg(".venv")
        .arg("-r")
        .arg(req.render_for(host.os()))
        .working_dir(HostPath::from_posix(&instance.install_dir));
    run_host_cmd(host, cmd, "uv pip install", log).await
}

async fn flatten_extract(host: &dyn Host, dest: &HostPath) -> Result<(), AppFrameworkError> {
    if host
        .exists(&dest.join("metadata.yaml"))
        .await
        .map_err(host_err)?
    {
        return Ok(());
    }
    let entries = host.list_dir(dest).await.map_err(host_err)?;
    let dirs: Vec<_> = entries
        .iter()
        .filter(|e| e.is_dir && !e.name.starts_with('.') && reject_unsafe_name(&e.name).is_ok())
        .collect();
    let [inner_entry] = dirs.as_slice() else {
        return Ok(());
    };
    let inner_name = inner_entry.name.clone();
    let inner = dest.join(&inner_name);
    if !host
        .exists(&inner.join("metadata.yaml"))
        .await
        .map_err(host_err)?
    {
        return Ok(());
    }
    let flatten_err = |e: HostError| AppFrameworkError::Runtime(format!("无法展开 zip 目录: {e}"));
    let children = host.list_dir(&inner).await.map_err(flatten_err)?;
    // 动手前先把每个子项过一遍：挪不了的名字不能等到最后跟着暂存目录一起被删掉。
    // 这里是 list_dir 给的单段文件名，notes..txt 这种名字里带 .. 的挪得动，不按市场目录名那么严
    for child in &children {
        if !is_movable_child_name(&child.name) {
            return Err(AppFrameworkError::Runtime(format!(
                "无法展开 zip 目录: 不支持的文件名 {}",
                child.name
            )));
        }
        if child.name != inner_name && entries.iter().any(|e| e.name == child.name) {
            return Err(AppFrameworkError::Runtime(format!(
                "无法展开 zip 目录: 顶层已有同名项 {}",
                child.name
            )));
        }
    }
    // 目录名来自插件作者打的 zip，只能当路径交给 Host 改名，不能拼进 shell 命令。
    // 先把顶层目录挪到暂存名，里面若有和它同名的子项也不会撞上它自己；
    // 暂存名避开顶层和子项里已有的名字，zip 里自带一个同名目录也不会被当成暂存目录删掉
    let taken = |name: &str| entries.iter().chain(&children).any(|e| e.name == name);
    let mut staging_name = String::from(".ncd-flatten");
    let mut n = 0u32;
    while taken(&staging_name) {
        n += 1;
        staging_name = format!(".ncd-flatten-{n}");
    }
    let staging = dest.join(&staging_name);
    host.rename(&inner, &staging).await.map_err(flatten_err)?;
    for child in &children {
        let target = dest.join(&child.name);
        // 远端 mv 遇到已存在的目录会挪进去而不是报错，先查一遍免得套成两层
        if host.exists(&target).await.map_err(flatten_err)? {
            return Err(AppFrameworkError::Runtime(format!(
                "无法展开 zip 目录: 顶层已有同名项 {}",
                child.name
            )));
        }
        host.rename(&staging.join(&child.name), &target)
            .await
            .map_err(flatten_err)?;
    }
    host.remove_dir_all(&staging).await.map_err(flatten_err)?;
    Ok(())
}

pub async fn install_item(
    host: &dyn Host,
    instance: &AppInstance,
    entry: &AppStoreMarketEntry,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    if entry.resource != AppStoreResource::Plugin {
        return Err(AppFrameworkError::PluginUnsupported(
            "AstrBot 没有平台适配器商店".into(),
        ));
    }
    let name = plugin_dir_name(entry)?;
    let dest = plugins_root(instance).join(&name);
    if host.exists(&dest).await.map_err(host_err)? {
        return Err(AppFrameworkError::Validation(format!("已存在: {name}")));
    }
    host.create_dir_all(&plugins_root(instance))
        .await
        .map_err(host_err)?;

    let zip_url = entry
        .files
        .first()
        .map(|f| f.url.as_str())
        .filter(|s| !s.is_empty());
    if let Some(url) = zip_url {
        let zip = plugins_root(instance).join(format!(".ncd-{name}.zip"));
        download_file_with_mirrors(host, url, &zip, log).await?;
        host.create_dir_all(&dest).await.map_err(host_err)?;
        let unpacked = async {
            host.extract_archive(&zip, &dest, ArchiveKind::Zip)
                .await
                .map_err(host_err)?;
            flatten_extract(host, &dest).await?;
            confirm_metadata(host, &dest, entry).await
        }
        .await;
        let _ = host.remove_file(&zip).await;
        if let Err(err) = unpacked {
            // 解到一半的目录留着的话，重试一上来就报「已存在」，更新也会一直撞同一个错
            let _ = host.remove_dir_all(&dest).await;
            return Err(err);
        }
    } else if !entry.package.is_empty() {
        let urls = github_clone_candidates(&entry.package)?;
        git_clone_try(host, &dest, &urls, log).await?;
        confirm_metadata(host, &dest, entry).await?;
    } else {
        return Err(AppFrameworkError::Validation(
            "条目没有 download_url 也没有 repo".into(),
        ));
    }
    pip_requirements(host, instance, &dest, log).await?;
    Ok(())
}

pub async fn update_item(
    host: &dyn Host,
    instance: &AppInstance,
    entry: &AppStoreMarketEntry,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    let name = plugin_dir_name(entry)?;
    let dest = plugins_root(instance).join(&name);
    if !host.exists(&dest).await.map_err(host_err)? {
        return install_item(host, instance, entry, log).await;
    }
    if host.exists(&dest.join(".git")).await.map_err(host_err)? && host.command_exists("git").await
    {
        emit_log(log, format!("git pull --ff-only {name}"));
        let cmd = HostCommand::new("git")
            .arg("-C")
            .arg(dest.render_for(host.os()))
            .arg("pull")
            .arg("--ff-only");
        run_host_cmd(host, cmd, "git pull", log).await?;
    } else {
        host.remove_dir_all(&dest).await.map_err(host_err)?;
        install_item(host, instance, entry, log).await?;
        return Ok(());
    }
    confirm_metadata(host, &dest, entry).await?;
    pip_requirements(host, instance, &dest, log).await?;
    Ok(())
}

pub async fn uninstall_item(
    host: &dyn Host,
    instance: &AppInstance,
    id: &str,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    let name = id.rsplit('/').next().unwrap_or(id);
    reject_unsafe_name(name)?;
    let dest = plugins_root(instance).join(name);
    if !host.exists(&dest).await.map_err(host_err)? {
        return Err(AppFrameworkError::Validation(format!("未安装: {id}")));
    }
    emit_log(log, format!("删除 data/plugins/{name}"));
    host.remove_dir_all(&dest).await.map_err(host_err)?;
    let mut prefs = load_prefs(host, instance).await?;
    if let Some(arr) = prefs
        .get_mut("inactivated_plugins")
        .and_then(Value::as_array_mut)
    {
        arr.retain(|v| {
            v.as_str()
                .is_none_or(|p| !p.starts_with(&format!("data.plugins.{name}.")))
        });
    }
    save_prefs(host, instance, &prefs).await?;
    Ok(())
}

pub async fn set_enabled(
    host: &dyn Host,
    instance: &AppInstance,
    id: &str,
    enabled: bool,
) -> Result<(), AppFrameworkError> {
    let name = id.rsplit('/').next().unwrap_or(id);
    reject_unsafe_name(name)?;
    let dest = plugins_root(instance).join(name);
    if !host.exists(&dest).await.map_err(host_err)? {
        return Err(AppFrameworkError::Validation(format!("未安装: {id}")));
    }
    let has_main = host.exists(&dest.join("main.py")).await.map_err(host_err)?;
    let module = module_path_for(name, has_main);
    let mut prefs = load_prefs(host, instance).await?;
    let arr = prefs
        .as_object_mut()
        .ok_or_else(|| AppFrameworkError::Integration("shared_preferences 根必须是对象".into()))?
        .entry("inactivated_plugins".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    let arr = arr
        .as_array_mut()
        .ok_or_else(|| AppFrameworkError::Integration("inactivated_plugins 必须是数组".into()))?;
    if enabled {
        arr.retain(|v| v.as_str() != Some(module.as_str()));
    } else if !arr.iter().any(|v| v.as_str() == Some(module.as_str())) {
        arr.push(Value::String(module));
    }
    save_prefs(host, instance, &prefs).await
}

#[cfg(test)]
mod tests {
    use super::*;

    const WITH_META: &str = r#"{
      "$meta": { "schema_version": 1 },
      "soulter/helloworld": {
        "author": "soulter",
        "name": "helloworld",
        "desc": "demo",
        "version": "1.2.0",
        "repo": "https://github.com/Soulter/helloworld",
        "tags": ["fun"],
        "support_platforms": ["aiocqhttp"]
      }
    }"#;

    const LEGACY: &str = r#"{
      "foo": {
        "author": "a",
        "name": "foo",
        "description": "old",
        "version": "0.1",
        "repo": "https://github.com/a/foo"
      }
    }"#;

    #[test]
    fn parse_skips_meta_and_reads_plugin_id() {
        let list = parse_astrbot_plugins_json(WITH_META).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "soulter/helloworld");
        assert_eq!(list[0].flavor, AppStoreFlavor::Git);
        assert_eq!(list[0].resource, AppStoreResource::Plugin);
        assert_eq!(list[0].package, "https://github.com/Soulter/helloworld");
        assert_eq!(list[0].supported_adapters, vec!["aiocqhttp".to_string()]);
    }

    #[test]
    fn parse_legacy_without_meta() {
        let list = parse_astrbot_plugins_json(LEGACY).unwrap();
        assert_eq!(list[0].id, "a/foo");
        assert_eq!(list[0].description, "old");
    }

    #[test]
    fn metadata_yaml_reads_author_name() {
        let m = parse_metadata_yaml("name: helloworld\nauthor: soulter\nversion: 1.0.0\n");
        assert_eq!(m.plugin_id(), "soulter/helloworld");
    }

    // astrbot_plugin_group_chat_plus 的 metadata.yaml 每个字段都带行内注释，身份校验曾因此误判不符
    #[test]
    fn metadata_yaml_strips_inline_comments() {
        let text = "name: astrbot_plugin_group_chat_plus # 插件唯一识别名\n\
                    author: Him666233 # 作者\n\
                    repo: https://github.com/Him666233/astrbot_plugin_group_chat_plus # 插件的仓库地址\n";
        let m = parse_metadata_yaml(text);
        assert_eq!(m.plugin_id(), "Him666233/astrbot_plugin_group_chat_plus");
        assert_eq!(
            m.repo,
            "https://github.com/Him666233/astrbot_plugin_group_chat_plus"
        );
    }

    #[test]
    fn metadata_yaml_keeps_hash_in_quotes_and_plain_scalars() {
        let m = parse_metadata_yaml("name: \"a # b\"\ndesc: sharp#tag\nversion: '1.0' # v\n");
        assert_eq!(m.name, "a # b");
        assert_eq!(m.desc, "sharp#tag");
        assert_eq!(m.version, "1.0");
    }

    #[test]
    fn module_path_matches_upstream() {
        assert_eq!(module_path_for("demo", true), "data.plugins.demo.main");
        assert_eq!(
            module_path_for("weather", false),
            "data.plugins.weather.weather"
        );
    }

    #[test]
    fn github_only_https() {
        assert!(github_clone_candidates("https://github.com/a/b").is_ok());
        assert!(github_clone_candidates("git@github.com:a/b.git").is_err());
    }

    #[test]
    fn zip_mirrors_github_and_leaves_other_hosts() {
        let gh = github_download_candidates("https://github.com/a/b/archive/main.zip");
        assert_eq!(gh[0], "https://github.com/a/b/archive/main.zip");
        assert!(gh.iter().any(|u| u.contains("gh-proxy.com")));
        let other = github_download_candidates("https://example.com/a.zip");
        assert_eq!(other, vec!["https://example.com/a.zip".to_string()]);
    }

    // 顶层目录名是 zip 作者定的；带 shell 元字符也只能被当成普通名字挪走
    #[cfg(windows)]
    #[tokio::test]
    async fn flatten_moves_children_without_shell() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("plugin");
        let inner = dest.join("x$(touch pwned)`id`");
        std::fs::create_dir_all(inner.join("x$(touch pwned)`id`")).unwrap();
        std::fs::write(inner.join("metadata.yaml"), b"name: demo\n").unwrap();
        std::fs::write(inner.join(".env.example"), b"A=1\n").unwrap();

        let host = ncd_host::local::LocalWindowsHost::new();
        let dest_path = HostPath::from_windows(dest.to_str().unwrap());
        flatten_extract(&host, &dest_path).await.unwrap();

        assert!(dest.join("metadata.yaml").is_file());
        assert!(dest.join(".env.example").is_file());
        assert!(dest.join("x$(touch pwned)`id`").is_dir());
        assert!(!dest.join(".ncd-flatten").exists());
        assert!(!tmp.path().join("pwned").exists());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn flatten_leaves_already_flat_plugin_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("plugin");
        std::fs::create_dir_all(dest.join("sub")).unwrap();
        std::fs::write(dest.join("metadata.yaml"), b"name: demo\n").unwrap();

        let host = ncd_host::local::LocalWindowsHost::new();
        flatten_extract(&host, &HostPath::from_windows(dest.to_str().unwrap()))
            .await
            .unwrap();
        assert!(dest.join("sub").is_dir());
    }

    // zip 顶层自带一个和暂存名同名的目录：换个暂存名，自带的那个原样留着
    #[cfg(windows)]
    #[tokio::test]
    async fn flatten_staging_avoids_existing_names() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("plugin");
        std::fs::create_dir_all(dest.join(".ncd-flatten")).unwrap();
        std::fs::write(dest.join(".ncd-flatten").join("keep.txt"), b"k").unwrap();
        let inner = dest.join("demo-main");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(inner.join("metadata.yaml"), b"name: demo\n").unwrap();
        std::fs::write(inner.join("main.py"), b"pass\n").unwrap();

        let host = ncd_host::local::LocalWindowsHost::new();
        flatten_extract(&host, &HostPath::from_windows(dest.to_str().unwrap()))
            .await
            .unwrap();

        assert!(dest.join("metadata.yaml").is_file());
        assert!(dest.join("main.py").is_file());
        assert!(dest.join(".ncd-flatten").join("keep.txt").is_file());
        assert!(!dest.join(".ncd-flatten-1").exists());
        assert!(!inner.exists());
    }

    // 文件名里带 .. 只是普通单段名字，照常挪上来，不因此让整个插件装不上
    #[cfg(windows)]
    #[tokio::test]
    async fn flatten_moves_names_with_double_dots() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("plugin");
        let inner = dest.join("demo-main");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(inner.join("metadata.yaml"), b"name: demo\n").unwrap();
        std::fs::write(inner.join("notes..txt"), b"n").unwrap();

        let host = ncd_host::local::LocalWindowsHost::new();
        flatten_extract(&host, &HostPath::from_windows(dest.to_str().unwrap()))
            .await
            .unwrap();

        assert!(dest.join("notes..txt").is_file());
        assert!(dest.join("metadata.yaml").is_file());
        assert!(!inner.exists());
    }

    #[test]
    fn child_names_that_cannot_move() {
        for bad in ["", ".", "..", "a/b", "a\\b"] {
            assert!(!is_movable_child_name(bad), "{bad:?}");
        }
        for ok in ["notes..txt", "v1..2.md", ".env", "main.py"] {
            assert!(is_movable_child_name(ok), "{ok:?}");
        }
    }
}
