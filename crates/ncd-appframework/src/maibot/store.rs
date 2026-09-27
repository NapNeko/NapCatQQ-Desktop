//! 插件商店：市场是官方 `Mai-with-u/plugin-repo` 的 `plugin_details.json`，装在
//! `plugins/<插件 id 的 . 换成 _>/`（和上游 WebUI 装的位置一致，互相认得）。
//!
//! 麦麦的插件运行时监听 `plugins/`、跳过点开头的目录：先在 `plugins/.ncd-stage/` 里下载、解压、
//! 校验好，再一次改名挪进去，运行中装、更、卸都能热生效，不会读到半截文件。
//! 插件的 Python 依赖由麦麦载入时自己装，这里只管文件。
//! 远端实例的插件包由桌面端下好写上去（服务器常常连不上 GitHub），用 tar.gz（服务器上 tar 必有、unzip 常缺）。

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ncd_domain::{AppConfigDocument, AppConfigFormat, AppInstance, AppInstanceState, AppStoreResource};
use ncd_host::{ArchiveKind, Host, HostError, HostPath, Locality};
use ncd_traits::AppFrameworkError;
use serde_json::{Map, Value};

use crate::adapter::PluginLogSink;
use crate::astrbot::store::download_file_with_mirrors;
use crate::store::{AppStoreFlavor, AppStoreInstalled, AppStoreMarketEntry};

pub const PLUGINS_DIR: &str = "plugins";
const STAGE: &str = ".ncd-stage";
const MANIFEST: &str = "_manifest.json";
const PLUGIN_CONFIG: &str = "config.toml";
const MARKET_RAW: &str = "https://raw.githubusercontent.com/Mai-with-u/plugin-repo/main/plugin_details.json";

/// 桌面端装实例时放进去、对接时写配置的 NapCat 适配器：商店里只读，
/// 换版本得跟着 MaiBot 本体走（它的清单卡着宿主版本范围）
pub const LOCKED_PLUGIN_ID: &str = "maibot-team.napcat-adapter";
const LOCKED_PLUGIN_DIR: &str = "MaiBot-Napcat-Adapter";
/// 1.2 的插件运行时只收第二版清单，老插件装上也载不进来
const MANIFEST_VERSION: u64 = 2;

pub const PLUGIN_DOC_PREFIX: &str = "plugin:";

pub fn maibot_plugin_market_urls() -> Vec<String> {
    vec![
        MARKET_RAW.to_string(),
        format!("https://gh-proxy.com/{MARKET_RAW}"),
        "https://cdn.jsdelivr.net/gh/Mai-with-u/plugin-repo@main/plugin_details.json".to_string(),
    ]
}

fn host_err(e: HostError) -> AppFrameworkError {
    AppFrameworkError::Host(e.to_string())
}

fn text(obj: &Map<String, Value>, key: &str) -> String {
    obj.get(key).and_then(Value::as_str).unwrap_or("").trim().to_string()
}

fn first_text(obj: &Map<String, Value>, keys: &[&str]) -> String {
    keys.iter().map(|k| text(obj, k)).find(|s| !s.is_empty()).unwrap_or_default()
}

fn repository_of(manifest: &Map<String, Value>) -> String {
    let direct = text(manifest, "repository_url");
    if !direct.is_empty() {
        return direct;
    }
    manifest
        .get("urls")
        .and_then(Value::as_object)
        .map(|u| text(u, "repository"))
        .unwrap_or_default()
}

fn is_locked(id: &str, dir: &str) -> bool {
    id.eq_ignore_ascii_case(LOCKED_PLUGIN_ID) || dir == LOCKED_PLUGIN_DIR
}

/// 同上游 `validate_plugin_id`：id 会拿去当目录名，不能带路径分隔、`..`、首尾点
pub fn validate_plugin_id(id: &str) -> Result<&str, AppFrameworkError> {
    let id = id.trim();
    let bad = id.is_empty()
        || ["/", "\\", "\0", "..", "\n", "\r", "\t"].iter().any(|p| id.contains(p))
        || id.starts_with('.')
        || id.ends_with('.');
    if bad {
        return Err(AppFrameworkError::Validation(format!("插件 id 不合法：{id:?}")));
    }
    Ok(id)
}

/// 和上游 `get_plugin_candidate_paths` 一样把点换成下划线，WebUI 装的和这里装的落在同一处
pub fn plugin_dir_for(id: &str) -> String {
    id.replace('.', "_")
}

pub fn parse_maibot_plugins_json(text_body: &str) -> Result<Vec<AppStoreMarketEntry>, AppFrameworkError> {
    let root: Value = serde_json::from_str(text_body)
        .map_err(|e| AppFrameworkError::Validation(format!("解析 MaiBot 插件目录失败: {e}")))?;
    let items = root
        .as_array()
        .ok_or_else(|| AppFrameworkError::Validation("MaiBot 插件目录根必须是数组".into()))?;
    let mut out = Vec::new();
    for item in items {
        let Some(item) = item.as_object() else { continue };
        let Some(m) = item.get("manifest").and_then(Value::as_object) else { continue };
        if m.get("manifest_version").and_then(Value::as_u64) != Some(MANIFEST_VERSION) {
            continue;
        }
        // 上游 WebUI 以清单里的 id 为准，外层 id 只是市场自己的编号（大小写还可能不一样）
        let id = first_text(m, &["id"]);
        let id = if id.is_empty() { text(item, "id") } else { id };
        let name = text(m, "name");
        let version = text(m, "version");
        let repo = repository_of(m);
        if validate_plugin_id(&id).is_err() || name.is_empty() || version.is_empty() || repo.is_empty() {
            continue;
        }
        let author = match m.get("author") {
            Some(Value::Object(a)) => text(a, "name"),
            Some(Value::String(s)) => s.trim().to_string(),
            _ => String::new(),
        };
        let homepage = {
            let h = first_text(m, &["homepage_url"]);
            let from_urls = m.get("urls").and_then(Value::as_object).map(|u| text(u, "homepage"));
            if !h.is_empty() { h } else { from_urls.filter(|s| !s.is_empty()).unwrap_or_else(|| repo.clone()) }
        };
        let tags = m
            .get("keywords")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect())
            .unwrap_or_default();
        out.push(AppStoreMarketEntry {
            resource: AppStoreResource::Plugin,
            is_official: id.starts_with("maibot-team."),
            id,
            name,
            description: text(m, "description"),
            version,
            author,
            homepage,
            time: String::new(),
            package: repo,
            module_name: String::new(),
            flavor: AppStoreFlavor::Git,
            valid: true,
            tags,
            supported_adapters: Vec::new(),
            authors: Vec::new(),
            repos: Vec::new(),
            files: Vec::new(),
            allow_build: Vec::new(),
        });
    }
    Ok(out)
}

pub(super) fn plugins_root(instance: &AppInstance) -> HostPath {
    HostPath::from_posix(&instance.install_dir).join(PLUGINS_DIR)
}

/// 上游的保留目录：`data`、`__pycache__`、点开头的（不区分大小写）
fn is_reserved(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n == "data" || n == "__pycache__" || n.starts_with('.')
}

async fn read_text(host: &dyn Host, path: &HostPath) -> Result<Option<String>, AppFrameworkError> {
    if !host.exists(path).await.map_err(host_err)? {
        return Ok(None);
    }
    let bytes = host.read_file(path).await.map_err(host_err)?;
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

async fn read_manifest(host: &dyn Host, dir: &HostPath) -> Result<Option<Map<String, Value>>, AppFrameworkError> {
    let Some(raw) = read_text(host, &dir.join(MANIFEST)).await? else {
        return Ok(None);
    };
    Ok(serde_json::from_str::<Value>(&raw).ok().and_then(|v| v.as_object().cloned()))
}

/// 同上游 `_read_plugin_enabled`：没配置文件、没 `[plugin]`、没 `enabled` 都算开着；字符串按常见的否定词认
pub fn plugin_enabled(config_toml: Option<&str>) -> bool {
    let Some(table) = config_toml.and_then(|t| t.parse::<toml::Table>().ok()) else {
        return true;
    };
    match table.get("plugin").and_then(|p| p.get("enabled")) {
        Some(toml::Value::Boolean(b)) => *b,
        Some(toml::Value::String(s)) => {
            !matches!(s.trim().to_ascii_lowercase().as_str(), "false" | "0" | "no" | "off" | "disabled")
        }
        Some(toml::Value::Integer(i)) => *i != 0,
        _ => true,
    }
}

/// 按清单 id 找已装的目录名（大小写不敏感，同上游 `find_plugin_path_by_id`）。目录名不一定是 id 换出来的
pub(super) async fn find_installed_dir(
    host: &dyn Host,
    root: &HostPath,
    id: &str,
) -> Result<Option<String>, AppFrameworkError> {
    if !host.exists(root).await.map_err(host_err)? {
        return Ok(None);
    }
    let mut folded = None;
    for entry in host.list_dir(root).await.map_err(host_err)? {
        if !entry.is_dir || is_reserved(&entry.name) {
            continue;
        }
        let Some(m) = read_manifest(host, &root.join(&entry.name)).await? else { continue };
        let got = text(&m, "id");
        if got == id {
            return Ok(Some(entry.name));
        }
        if folded.is_none() && got.eq_ignore_ascii_case(id) {
            folded = Some(entry.name);
        }
    }
    Ok(folded)
}

pub async fn list_installed(
    host: &dyn Host,
    instance: &AppInstance,
    resource: AppStoreResource,
) -> Result<Vec<AppStoreInstalled>, AppFrameworkError> {
    let root = plugins_root(instance);
    if resource != AppStoreResource::Plugin || !host.exists(&root).await.map_err(host_err)? {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in host.list_dir(&root).await.map_err(host_err)? {
        if !entry.is_dir || is_reserved(&entry.name) {
            continue;
        }
        let dir = root.join(&entry.name);
        // 没有清单的是卸载残留（只剩 config.toml）或者根本不是插件，上游也不认
        let Some(m) = read_manifest(host, &dir).await? else { continue };
        let id = text(&m, "id");
        let id = if id.is_empty() { entry.name.clone() } else { id };
        let name = text(&m, "name");
        let version = text(&m, "version");
        let config = read_text(host, &dir.join(PLUGIN_CONFIG)).await?;
        out.push(AppStoreInstalled {
            locked: is_locked(&id, &entry.name),
            name: if name.is_empty() { entry.name.clone() } else { name },
            id,
            resource: AppStoreResource::Plugin,
            flavor: AppStoreFlavor::Git,
            version: (!version.is_empty()).then_some(version),
            enabled: plugin_enabled(config.as_deref()),
            package: repository_of(&m),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn emit(log: Option<&PluginLogSink>, line: impl Into<String>) {
    if let Some(sink) = log {
        sink(line.into());
    }
}

fn refuse_locked(id: &str, dir: &str) -> Result<(), AppFrameworkError> {
    if is_locked(id, dir) {
        return Err(AppFrameworkError::Validation(
            "NapCat 适配器由桌面端管理（装实例时放进去、对接时写配置），这里不能卸、停或更新".into(),
        ));
    }
    Ok(())
}

/// 只接受 GitHub 仓库：下载走 `archive/HEAD.<ext>`（跟默认分支），不依赖主机上有 git
fn archive_url(repository: &str, ext: &str) -> Result<String, AppFrameworkError> {
    let repo = repository.trim().trim_end_matches('/').trim_end_matches(".git");
    let rest = repo
        .strip_prefix("https://github.com/")
        .ok_or_else(|| AppFrameworkError::Validation(format!("只装得了 GitHub 上的插件：{repository}")))?;
    let mut parts = rest.split('/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(owner), Some(name), None) if !owner.is_empty() && !name.is_empty() => {
            Ok(format!("https://github.com/{owner}/{name}/archive/HEAD.{ext}"))
        }
        _ => Err(AppFrameworkError::Validation(format!("认不出仓库地址：{repository}"))),
    }
}

/// 本机下 zip（进程内解压）；远端下 tar.gz：服务器上 tar 必有、unzip 常缺，缺了还得 sudo 装
fn archive_format(locality: Locality) -> (&'static str, ArchiveKind) {
    match locality {
        Locality::Local => ("zip", ArchiveKind::Zip),
        Locality::Remote => ("tar.gz", ArchiveKind::TarGz),
    }
}

/// 插件包就几 MB，整包读进内存再写上去
const DESKTOP_FETCH_TIMEOUT: Duration = Duration::from_secs(120);

/// 远端实例的插件包由桌面端按 GitHub 镜像表挨个试着下
async fn fetch_on_desktop(url: &str, log: Option<&PluginLogSink>) -> Result<Vec<u8>, AppFrameworkError> {
    let mut last = String::from("没有可用下载地址");
    for candidate in ncd_network::build_mirror_urls(url, None) {
        emit(log, format!("桌面端下载 {candidate}"));
        let sent = ncd_network::shared_client()
            .get(&candidate)
            .timeout(DESKTOP_FETCH_TIMEOUT)
            .send()
            .await;
        match sent {
            Ok(resp) if resp.status().is_success() => match resp.bytes().await {
                Ok(bytes) => return Ok(bytes.to_vec()),
                Err(e) => last = e.without_url().to_string(),
            },
            Ok(resp) => last = format!("返回 {}", resp.status()),
            Err(e) => last = e.without_url().to_string(),
        }
    }
    Err(AppFrameworkError::Host(format!("插件包下载失败：{last}")))
}

fn stamp() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

async fn new_stage(host: &dyn Host, root: &HostPath, id: &str) -> Result<HostPath, AppFrameworkError> {
    let stage = root.join(STAGE).join(format!("{}-{}", plugin_dir_for(id), stamp()));
    host.create_dir_all(&stage).await.map_err(host_err)?;
    Ok(stage)
}

/// 把暂存目录里下好的包解开、找到插件根、核对清单。
/// GitHub 的包带一层 `<仓库>-<分支>/` 顶层目录，清单在它里面
async fn unpack_stage(
    host: &dyn Host,
    stage: &HostPath,
    archive: &HostPath,
    kind: ArchiveKind,
    id: &str,
    log: Option<&PluginLogSink>,
) -> Result<HostPath, AppFrameworkError> {
    let unpacked = stage.join("src");
    host.create_dir_all(&unpacked).await.map_err(host_err)?;
    emit(log, "解压");
    host.extract_archive(archive, &unpacked, kind)
        .await
        .map_err(host_err)?;
    let plugin_root = if host.exists(&unpacked.join(MANIFEST)).await.map_err(host_err)? {
        unpacked
    } else {
        let dirs: Vec<_> = host
            .list_dir(&unpacked)
            .await
            .map_err(host_err)?
            .into_iter()
            .filter(|e| e.is_dir)
            .collect();
        match dirs.as_slice() {
            [only] if host.exists(&unpacked.join(&only.name).join(MANIFEST)).await.map_err(host_err)? => {
                unpacked.join(&only.name)
            }
            _ => return Err(AppFrameworkError::Validation("包里没有 _manifest.json，不是 MaiBot 插件".into())),
        }
    };
    let m = read_manifest(host, &plugin_root)
        .await?
        .ok_or_else(|| AppFrameworkError::Validation("_manifest.json 读不出来".into()))?;
    if m.get("manifest_version").and_then(Value::as_u64) != Some(MANIFEST_VERSION) {
        return Err(AppFrameworkError::Validation(
            "这个插件是老版清单，当前 MaiBot 载不进来，等作者更新".into(),
        ));
    }
    let got = text(&m, "id");
    if !got.eq_ignore_ascii_case(id) {
        return Err(AppFrameworkError::Validation(format!("插件身份不符：包里是 {got:?}，市场上是 {id:?}")));
    }
    Ok(plugin_root)
}

/// 下载解压到暂存目录并核对清单，返回（插件根目录，要清理的暂存目录）；出错时暂存目录已清掉
async fn stage_plugin(
    host: &dyn Host,
    root: &HostPath,
    entry: &AppStoreMarketEntry,
    id: &str,
    log: Option<&PluginLogSink>,
) -> Result<(HostPath, HostPath), AppFrameworkError> {
    let (ext, kind) = archive_format(host.locality());
    let url = archive_url(&entry.package, ext)?;
    let stage = new_stage(host, root, id).await?;
    let archive = stage.join(format!("src.{ext}"));
    let result = async {
        match host.locality() {
            Locality::Local => download_file_with_mirrors(host, &url, &archive, log).await?,
            Locality::Remote => {
                let bytes = fetch_on_desktop(&url, log).await?;
                host.write_file(&archive, &bytes).await.map_err(host_err)?;
            }
        }
        unpack_stage(host, &stage, &archive, kind, id, log).await
    }
    .await;
    match result {
        Ok(plugin_root) => Ok((plugin_root, stage)),
        Err(e) => {
            let _ = host.remove_dir_all(&stage).await;
            Err(e)
        }
    }
}

/// 新装：一次改名挪进去，暂存目录不论成败都清掉
async fn place(host: &dyn Host, plugin_root: &HostPath, dest: &HostPath, stage: &HostPath) -> Result<(), AppFrameworkError> {
    let moved = host.rename(plugin_root, dest).await.map_err(host_err);
    let _ = host.remove_dir_all(stage).await;
    moved
}

/// 更新：带上用户配置，旧的先挪进暂存、新的挪到原位，挪新的失败就把旧的放回去
async fn swap_in(
    host: &dyn Host,
    current: &HostPath,
    plugin_root: &HostPath,
    stage: &HostPath,
) -> Result<(), AppFrameworkError> {
    let result = async {
        carry_user_files(host, current, plugin_root).await?;
        let old = stage.join("old");
        host.rename(current, &old).await.map_err(host_err)?;
        if let Err(e) = host.rename(plugin_root, current).await {
            let _ = host.rename(&old, current).await;
            return Err(host_err(e));
        }
        Ok(())
    }
    .await;
    let _ = host.remove_dir_all(stage).await;
    result
}

/// 只剩 `config.toml` 的目录是上游卸载失败的残留，装之前清掉（同上游 `is_plugin_install_residue`）
async fn clear_residue(host: &dyn Host, dir: &HostPath) -> Result<bool, AppFrameworkError> {
    if !host.exists(dir).await.map_err(host_err)? {
        return Ok(true);
    }
    let entries = host.list_dir(dir).await.map_err(host_err)?;
    if entries.iter().all(|e| !e.is_dir && e.name == PLUGIN_CONFIG) {
        host.remove_dir_all(dir).await.map_err(host_err)?;
        return Ok(true);
    }
    Ok(false)
}

pub async fn install_item(
    host: &dyn Host,
    instance: &AppInstance,
    entry: &AppStoreMarketEntry,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    let id = validate_plugin_id(&entry.id)?;
    let dir = plugin_dir_for(id);
    refuse_locked(id, &dir)?;
    let root = plugins_root(instance);
    if let Some(found) = find_installed_dir(host, &root, id).await? {
        return Err(AppFrameworkError::Validation(format!("已经装了（plugins/{found}）")));
    }
    let dest = root.join(&dir);
    if !clear_residue(host, &dest).await? {
        return Err(AppFrameworkError::Validation(format!("plugins/{dir} 已被别的东西占着")));
    }
    let (plugin_root, stage) = stage_plugin(host, &root, entry, id, log).await?;
    place(host, &plugin_root, &dest, &stage).await?;
    emit(log, format!("已放进 plugins/{dir}，麦麦在跑的话会自己载入并装依赖"));
    Ok(())
}

/// 用户改过的只有配置：新版本换进来前把 `config.toml` 和上游 WebUI 的配置备份目录带过去
async fn carry_user_files(host: &dyn Host, from: &HostPath, to: &HostPath) -> Result<(), AppFrameworkError> {
    if let Some(cfg) = read_text(host, &from.join(PLUGIN_CONFIG)).await? {
        host.write_file(&to.join(PLUGIN_CONFIG), cfg.as_bytes()).await.map_err(host_err)?;
    }
    let backups = from.join("config_back");
    if host.exists(&backups).await.map_err(host_err)? {
        let target = to.join("config_back");
        host.create_dir_all(&target).await.map_err(host_err)?;
        for e in host.list_dir(&backups).await.map_err(host_err)? {
            if e.is_dir {
                continue;
            }
            let bytes = host.read_file(&backups.join(&e.name)).await.map_err(host_err)?;
            host.write_file(&target.join(&e.name), &bytes).await.map_err(host_err)?;
        }
    }
    Ok(())
}

pub async fn update_item(
    host: &dyn Host,
    instance: &AppInstance,
    entry: &AppStoreMarketEntry,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    let id = validate_plugin_id(&entry.id)?;
    let root = plugins_root(instance);
    let Some(dir) = find_installed_dir(host, &root, id).await? else {
        return install_item(host, instance, entry, log).await;
    };
    refuse_locked(id, &dir)?;
    let (plugin_root, stage) = stage_plugin(host, &root, entry, id, log).await?;
    swap_in(host, &root.join(&dir), &plugin_root, &stage).await?;
    emit(log, format!("plugins/{dir} 换成了 {}，配置原样保留", entry.version));
    Ok(())
}

/// 改 `[plugin].enabled`，其余内容和注释原样。没有配置文件就只写这一项（上游切换时也是这么补的）
pub fn set_enabled_in(config_toml: Option<&str>, enabled: bool) -> Result<String, AppFrameworkError> {
    let mut doc = config_toml
        .unwrap_or("")
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| AppFrameworkError::Validation(format!("插件的 config.toml 不是合法的 TOML：{e}")))?;
    if !doc.contains_table("plugin") {
        doc["plugin"] = toml_edit::table();
    }
    doc["plugin"]["enabled"] = toml_edit::value(enabled);
    Ok(doc.to_string())
}

pub async fn set_enabled(
    host: &dyn Host,
    instance: &AppInstance,
    id: &str,
    enabled: bool,
) -> Result<(), AppFrameworkError> {
    let id = validate_plugin_id(id)?;
    let root = plugins_root(instance);
    let dir = find_installed_dir(host, &root, id)
        .await?
        .ok_or_else(|| AppFrameworkError::Validation(format!("没装：{id}")))?;
    refuse_locked(id, &dir)?;
    let path = root.join(&dir).join(PLUGIN_CONFIG);
    let current = read_text(host, &path).await?;
    let next = set_enabled_in(current.as_deref(), enabled)?;
    host.write_file(&path, next.as_bytes()).await.map_err(host_err)
}

pub async fn uninstall_item(
    host: &dyn Host,
    instance: &AppInstance,
    id: &str,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    let id = validate_plugin_id(id)?;
    let root = plugins_root(instance);
    let dir = find_installed_dir(host, &root, id)
        .await?
        .ok_or_else(|| AppFrameworkError::Validation(format!("没装：{id}")))?;
    refuse_locked(id, &dir)?;
    // 运行中先停用，让插件运行时卸下它（监听去抖 600ms），再删文件，免得删到一半它还在用
    if instance.state == AppInstanceState::Running {
        emit(log, "先停用，等麦麦卸下插件");
        set_enabled(host, instance, id, false).await?;
        tokio::time::sleep(Duration::from_millis(1500)).await;
    }
    emit(log, format!("删除 plugins/{dir}"));
    host.remove_dir_all(&root.join(&dir)).await.map_err(host_err)
}

pub fn plugin_doc_id(dir: &str) -> String {
    format!("{PLUGIN_DOC_PREFIX}{dir}")
}

pub fn parse_plugin_doc_id(doc_id: &str) -> Option<&str> {
    let dir = doc_id.strip_prefix(PLUGIN_DOC_PREFIX)?;
    if dir.is_empty() || dir.contains('/') || dir.contains('\\') || dir.contains("..") || is_reserved(dir) {
        return None;
    }
    Some(dir)
}

/// 插件自己的 `config.toml`：麦麦的插件运行时监听着它，改了热生效（上游 WebUI 存插件配置也只是写这个文件）
pub fn plugin_config_document(dir: &str) -> AppConfigDocument {
    AppConfigDocument {
        id: plugin_doc_id(dir),
        label: format!("{dir}/config.toml"),
        rel_path: format!("{PLUGINS_DIR}/{dir}/{PLUGIN_CONFIG}"),
        format: AppConfigFormat::Toml,
        hot_reload: true,
    }
}

pub async fn list_plugin_config_docs(
    host: &dyn Host,
    instance: &AppInstance,
    plugin_id: &str,
) -> Result<Vec<AppConfigDocument>, AppFrameworkError> {
    let id = validate_plugin_id(plugin_id)?;
    let dir = find_installed_dir(host, &plugins_root(instance), id)
        .await?
        .ok_or_else(|| AppFrameworkError::Validation(format!("没装：{id}")))?;
    Ok(vec![plugin_config_document(&dir)])
}

#[cfg(test)]
mod tests {
    use super::*;

    const MARKET: &str = r#"[
      {"id": "SengokuCola.Mute-Plugin", "manifest": {"manifest_version": 2, "version": "4.7.0",
        "name": "禁言插件", "description": "d", "author": {"name": "SengokuCola"},
        "urls": {"repository": "https://github.com/SengokuCola/MutePlugin"}, "keywords": ["群管", " "],
        "id": "sengokucola.mute-plugin"}},
      {"id": "old.plugin", "manifest": {"manifest_version": 1, "version": "1.0", "name": "老插件",
        "repository_url": "https://github.com/a/b"}},
      {"id": "maibot-team.napcat-adapter", "manifest": {"manifest_version": 2, "version": "1.4.0",
        "name": "Napcat 适配器", "author": "MaiBot Team", "repository_url": "https://github.com/Mai-with-u/MaiBot-Napcat-Adapter"}},
      {"id": "no.repo", "manifest": {"manifest_version": 2, "version": "1.0", "name": "没仓库"}},
      {"id": "../evil", "manifest": {"manifest_version": 2, "version": "1.0", "name": "x", "repository_url": "https://github.com/a/b"}}
    ]"#;

    #[test]
    fn market_keeps_installable_v2_entries_keyed_by_manifest_id() {
        let list = parse_maibot_plugins_json(MARKET).unwrap();
        let ids: Vec<&str> = list.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["sengokucola.mute-plugin", "maibot-team.napcat-adapter"], "老清单、没仓库、非法 id 都跳过");
        assert_eq!(list[0].author, "SengokuCola");
        assert_eq!(list[0].package, "https://github.com/SengokuCola/MutePlugin");
        assert_eq!(list[0].homepage, "https://github.com/SengokuCola/MutePlugin", "没主页就用仓库");
        assert_eq!(list[0].tags, vec!["群管".to_string()]);
        assert!(!list[0].is_official);
        assert_eq!(list[1].author, "MaiBot Team", "作者写成字符串也认");
        assert!(list[1].is_official);
    }

    #[test]
    fn plugin_dir_matches_upstream_and_ids_are_path_safe() {
        assert_eq!(plugin_dir_for("sengokucola.mute-plugin"), "sengokucola_mute-plugin");
        for bad in ["", "a/b", "a\\b", "..", ".hidden", "trailing.", "a..b"] {
            assert!(validate_plugin_id(bad).is_err(), "{bad:?}");
        }
        assert!(validate_plugin_id("author.name-plugin").is_ok());
    }

    #[test]
    fn enabled_follows_upstream_defaults() {
        assert!(plugin_enabled(None), "没配置文件算开着");
        assert!(plugin_enabled(Some("[other]\nx = 1\n")));
        assert!(!plugin_enabled(Some("[plugin]\nenabled = false\n")));
        assert!(!plugin_enabled(Some("[plugin]\nenabled = \"off\"\n")));
        assert!(plugin_enabled(Some("not toml [[[")), "读不懂按开着，同上游");
    }

    #[test]
    fn toggling_keeps_the_rest_of_the_file() {
        let before = "# 插件配置\n[plugin]\nenabled = true # 总开关\nname = \"x\"\n\n[extra]\nk = 1\n";
        let after = set_enabled_in(Some(before), false).unwrap();
        assert!(after.contains("enabled = false"), "{after}");
        assert!(after.contains("# 插件配置") && after.contains("[extra]\nk = 1"), "{after}");
        assert!(!plugin_enabled(Some(&after)));
        assert_eq!(set_enabled_in(None, false).unwrap().trim(), "[plugin]\nenabled = false");
    }

    #[test]
    fn archives_come_from_github_default_branch_only() {
        assert_eq!(
            archive_url("https://github.com/SengokuCola/MutePlugin.git/", "zip").unwrap(),
            "https://github.com/SengokuCola/MutePlugin/archive/HEAD.zip"
        );
        assert!(archive_url("https://gitee.com/a/b", "zip").is_err());
        assert!(archive_url("https://github.com/a", "zip").is_err());
        assert!(archive_url("https://github.com/a/b/tree/dev", "zip").is_err());
    }

    #[test]
    fn remote_hosts_get_tarballs() {
        assert_eq!(archive_format(Locality::Local), ("zip", ArchiveKind::Zip));
        assert_eq!(archive_format(Locality::Remote), ("tar.gz", ArchiveKind::TarGz), "服务器上 unzip 常缺");
        assert_eq!(
            archive_url("https://github.com/a/b", "tar.gz").unwrap(),
            "https://github.com/a/b/archive/HEAD.tar.gz"
        );
    }

    #[test]
    fn plugin_docs_reject_escapes_and_reserved_dirs() {
        assert_eq!(parse_plugin_doc_id("plugin:sengokucola_mute-plugin"), Some("sengokucola_mute-plugin"));
        for bad in ["plugin:", "plugin:../x", "plugin:a/b", "plugin:.ncd-stage", "plugin:data", "bot_config"] {
            assert_eq!(parse_plugin_doc_id(bad), None, "{bad:?}");
        }
        let doc = plugin_config_document("x");
        assert_eq!(doc.rel_path, "plugins/x/config.toml");
        assert!(doc.hot_reload);
    }

    #[test]
    fn the_napcat_adapter_is_locked_by_id_or_dir() {
        assert!(is_locked("maibot-team.napcat-adapter", "whatever"));
        assert!(is_locked("MaiBot-Team.Napcat-Adapter", "x"));
        assert!(is_locked("", "MaiBot-Napcat-Adapter"));
        assert!(!is_locked("sengokucola.mute-plugin", "sengokucola_mute-plugin"));
    }

    mod on_disk {
        use std::io::Write;

        use ncd_domain::{AppFrameworkId, AppInstanceId, AppPlacement};
        use ncd_host::local::LocalWindowsHost;

        use super::super::*;

        fn instance(dir: &std::path::Path, state: AppInstanceState) -> AppInstance {
            AppInstance {
                id: AppInstanceId::new("m1"),
                framework_id: AppFrameworkId::new("maibot"),
                display_name: "麦麦".into(),
                placement: AppPlacement::LocalNative,
                host_id: "local".into(),
                install_dir: dir.to_string_lossy().replace('\\', "/"),
                port: 23001,
                state,
                link: None,
                installed_version: None,
                last_error: None,
                created_at_ms: 1,
                install_renderer: false,
                origin: ncd_domain::AppInstanceOrigin::Created,
                auto_start: true,
            }
        }

        fn manifest(id: &str, version: &str) -> String {
            format!(r#"{{"manifest_version": 2, "id": "{id}", "name": "禁言", "version": "{version}", "urls": {{"repository": "https://github.com/a/mute"}}}}"#)
        }

        /// 仿 GitHub 源码包：一层 `<仓库>-<分支>/` 顶层目录
        fn write_zip(path: &std::path::Path, files: &[(&str, &str)]) {
            let file = std::fs::File::create(path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts = zip::write::SimpleFileOptions::default();
            for (name, body) in files {
                zip.start_file(format!("mute-main/{name}"), opts).unwrap();
                zip.write_all(body.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }

        async fn staged(host: &LocalWindowsHost, root: &HostPath, id: &str, files: &[(&str, &str)]) -> (HostPath, HostPath) {
            let stage = new_stage(host, root, id).await.unwrap();
            let plugin_root = unpack_zip(host, &stage, id, files).await.unwrap();
            (plugin_root, stage)
        }

        async fn unpack_zip(
            host: &LocalWindowsHost,
            stage: &HostPath,
            id: &str,
            files: &[(&str, &str)],
        ) -> Result<HostPath, AppFrameworkError> {
            let archive = stage.join("src.zip");
            write_zip(std::path::Path::new(&archive.as_posix()), files);
            unpack_stage(host, stage, &archive, ArchiveKind::Zip, id, None).await
        }

        #[tokio::test]
        async fn install_update_toggle_uninstall_round_trip() {
            let tmp = tempfile::tempdir().unwrap();
            let host = LocalWindowsHost::new();
            let inst = instance(tmp.path(), AppInstanceState::Installed);
            let root = plugins_root(&inst);
            let id = "sengokucola.mute-plugin";

            let (plugin_root, stage) = staged(&host, &root, id, &[("_manifest.json", &manifest(id, "1.0.0")), ("plugin.py", "v1")]).await;
            let dest = root.join(plugin_dir_for(id));
            place(&host, &plugin_root, &dest, &stage).await.unwrap();
            assert!(!host.exists(&stage).await.unwrap(), "暂存目录用完就清");

            let listed = list_installed(&host, &inst, AppStoreResource::Plugin).await.unwrap();
            assert_eq!(listed.len(), 1);
            assert_eq!((listed[0].id.as_str(), listed[0].version.as_deref(), listed[0].enabled), (id, Some("1.0.0"), true));
            assert_eq!(find_installed_dir(&host, &root, "SengokuCola.Mute-Plugin").await.unwrap().as_deref(), Some("sengokucola_mute-plugin"));

            set_enabled(&host, &inst, id, false).await.unwrap();
            std::fs::write(dest.join(PLUGIN_CONFIG).as_posix(), "[plugin]\nenabled = false\n\n[mute]\nminutes = 10\n").unwrap();

            let (plugin_root, stage) = staged(&host, &root, id, &[("_manifest.json", &manifest(id, "2.0.0")), ("plugin.py", "v2")]).await;
            swap_in(&host, &dest, &plugin_root, &stage).await.unwrap();
            assert_eq!(std::fs::read_to_string(dest.join("plugin.py").as_posix()).unwrap(), "v2");
            let cfg = std::fs::read_to_string(dest.join(PLUGIN_CONFIG).as_posix()).unwrap();
            assert!(cfg.contains("minutes = 10") && cfg.contains("enabled = false"), "更新保留用户配置：{cfg}");
            let listed = list_installed(&host, &inst, AppStoreResource::Plugin).await.unwrap();
            assert_eq!((listed[0].version.as_deref(), listed[0].enabled), (Some("2.0.0"), false));

            uninstall_item(&host, &inst, id, None).await.unwrap();
            assert!(list_installed(&host, &inst, AppStoreResource::Plugin).await.unwrap().is_empty());
        }

        #[tokio::test]
        async fn unpack_rejects_old_manifests_and_impostors() {
            let tmp = tempfile::tempdir().unwrap();
            let host = LocalWindowsHost::new();
            let root = plugins_root(&instance(tmp.path(), AppInstanceState::Installed));
            let id = "sengokucola.mute-plugin";

            let stage = new_stage(&host, &root, id).await.unwrap();
            let err = unpack_zip(&host, &stage, id, &[("_manifest.json", &manifest("someone.else", "1.0.0"))])
                .await
                .unwrap_err();
            assert!(err.to_string().contains("身份不符"), "{err}");

            let stage = new_stage(&host, &root, id).await.unwrap();
            let v1 = format!(r#"{{"manifest_version": 1, "id": "{id}", "name": "x", "version": "1"}}"#);
            let err = unpack_zip(&host, &stage, id, &[("_manifest.json", &v1)]).await.unwrap_err();
            assert!(err.to_string().contains("老版清单"), "{err}");

            let stage = new_stage(&host, &root, id).await.unwrap();
            assert!(unpack_zip(&host, &stage, id, &[("README.md", "no manifest")]).await.is_err());
        }

        /// 真下载：`archive/HEAD.zip` 要跟 GitHub 跳到 codeload，镜像也得能用。手动跑：
        /// cargo test -p ncd-appframework --lib real_market_install_smoke -- --ignored
        #[tokio::test]
        #[ignore = "要联网从 GitHub 下插件包"]
        async fn real_market_install_smoke() {
            let tmp = tempfile::tempdir().unwrap();
            let host = LocalWindowsHost::new();
            let inst = instance(tmp.path(), AppInstanceState::Installed);
            let text = reqwest::get(MARKET_RAW).await.unwrap().text().await.unwrap();
            let market = parse_maibot_plugins_json(&text).unwrap();
            let entry = market.iter().find(|e| e.id == "sengokucola.mute-plugin").expect("市场里有禁言插件");
            install_item(&host, &inst, entry, None).await.unwrap();
            let listed = list_installed(&host, &inst, AppStoreResource::Plugin).await.unwrap();
            assert_eq!(listed.len(), 1, "{listed:?}");
            assert_eq!(listed[0].id, entry.id);
            update_item(&host, &inst, entry, None).await.unwrap();
            uninstall_item(&host, &inst, &entry.id, None).await.unwrap();
            assert!(list_installed(&host, &inst, AppStoreResource::Plugin).await.unwrap().is_empty());
        }

        #[tokio::test]
        async fn the_managed_adapter_cannot_be_touched_and_residue_is_cleared() {
            let tmp = tempfile::tempdir().unwrap();
            let host = LocalWindowsHost::new();
            let inst = instance(tmp.path(), AppInstanceState::Installed);
            let root = plugins_root(&inst);
            let adapter = root.join(LOCKED_PLUGIN_DIR);
            std::fs::create_dir_all(adapter.as_posix()).unwrap();
            std::fs::write(adapter.join(MANIFEST).as_posix(), manifest(LOCKED_PLUGIN_ID, "1.4.0")).unwrap();

            let listed = list_installed(&host, &inst, AppStoreResource::Plugin).await.unwrap();
            assert!(listed[0].locked);
            assert!(set_enabled(&host, &inst, LOCKED_PLUGIN_ID, false).await.is_err());
            assert!(uninstall_item(&host, &inst, LOCKED_PLUGIN_ID, None).await.is_err());
            assert!(host.exists(&adapter).await.unwrap());

            // 上游卸载失败留下的只有 config.toml 的目录：装之前清掉
            let residue = root.join("sengokucola_mute-plugin");
            std::fs::create_dir_all(residue.as_posix()).unwrap();
            std::fs::write(residue.join(PLUGIN_CONFIG).as_posix(), "[plugin]\n").unwrap();
            assert!(clear_residue(&host, &residue).await.unwrap());
            assert!(!host.exists(&residue).await.unwrap());
            std::fs::create_dir_all(residue.as_posix()).unwrap();
            std::fs::write(residue.join("plugin.py").as_posix(), "x").unwrap();
            assert!(!clear_residue(&host, &residue).await.unwrap(), "有别的文件就不是残留，不动");
        }
    }
}
