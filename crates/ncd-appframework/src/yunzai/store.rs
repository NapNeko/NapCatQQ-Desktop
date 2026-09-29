//! 云崽插件商店：目录来自 yhArcadia/Yunzai-Bot-plugins-index 的五张 Markdown 表。
//!
//! - README 里「推荐插件」那张表记为推荐；功能 / 游戏IP / 文游 / 单JS 各一份文件，按分类打标签
//! - 行格式见索引的 CONTRIBUTING：`| [名称](主页) | [@作者](主页) | 备注 |`，单 JS 多一列源码链接
//! - 链接五花八门：相对路径（写给 Gitee 看的 `../../../../owner/repo`）、全角冒号、blob 页面链接、
//!   同一个文件给 GitHub / Gitee 两个地址，这里统一成能 clone 的仓库根 / 能直接下载的 raw 地址
//! - Gitee 的 raw 接口会把单 JS 那张表判成「内容可能违规」拒掉，所以每份都配了 Gitee API（base64）、
//!   GitHub raw、jsDelivr 几个源
//!
//! 装：目录插件 git clone 到 `plugins/<仓库名>`（目录名要和仓库名一致，喵喵这类插件互相按路径引用），
//! 有 package.json 再在实例根跑一次 pnpm install（`plugins/**` 是 workspace）；单 JS 由编排层下载到
//! `plugins/example/`。单 JS 能停用（改名成 `.js.disabled`，上游只加载 `.js`），目录插件不能单独停。

use std::time::Duration;

use base64::Engine;
use ncd_domain::{AppConfigDocument, AppConfigFormat, AppInstance, AppStoreResource};
use ncd_host::{Host, HostCommand, HostError, HostPath};
use ncd_traits::AppFrameworkError;

use super::git::{GIT_LONG_TIMEOUT, GitTool, clone_candidates, last_line};
use super::manifest::{YUNZAI_BUILTIN_PLUGIN_DIRS, YUNZAI_JS_PLUGIN_DIR, YUNZAI_PLUGINS_DIR};
use crate::adapter::PluginLogSink;
use crate::karin::plugin::{KarinPluginAppFile, KarinPluginAuthor, KarinPluginRepo};
use crate::node_tooling::{local_path_env, path_prefix, pnpm_command, read_node_marker, resolve_node_toolchain};
use crate::store::{
    AppStoreFlavor, AppStoreInstalled, AppStoreMarketEntry, StoreMarketPart, StoreMarketText,
    percent_decode_lossy,
};

const INDEX_OWNER_REPO: &str = "yhArcadia/Yunzai-Bot-plugins-index";
const PLUGIN_CMD_TIMEOUT: Duration = Duration::from_secs(20 * 60);
/// 单 JS 停用后的后缀
pub const DISABLED_SUFFIX: &str = ".disabled";
pub const YUNZAI_MARKET_CACHE_KEY: &str = "yunzai-plugins";
/// 装实例时记下的 git（托管 MinGit 的绝对路径）；商店装插件时拿不到组件目录，读这个
pub const GIT_MARKER_FILE: &str = ".ncd-git";

/// 索引的几份文件：(份 id, 文件名, 分类标签)
const PARTS: &[(&str, &str, &str)] = &[
    ("readme", "README.md", "推荐"),
    ("function", "Function-Plugin.md", "功能"),
    ("game", "Game-Plugin.md", "游戏"),
    ("wordgame", "WordGame-Plugin.md", "文游"),
    ("js", "JS-Plugin.md", "单 JS"),
];

/// 每份文件的镜像：Gitee API（base64 包在 JSON 里，绕开 raw 接口的内容审核）、GitHub raw、jsDelivr、Gitee raw
pub fn index_part_urls(file: &str) -> Vec<String> {
    vec![
        format!("https://gitee.com/api/v5/repos/{INDEX_OWNER_REPO}/contents/{file}?ref=main"),
        format!("https://raw.githubusercontent.com/{INDEX_OWNER_REPO}/main/{file}"),
        format!("https://cdn.jsdelivr.net/gh/{INDEX_OWNER_REPO}@main/{file}"),
        format!("https://gitee.com/{INDEX_OWNER_REPO}/raw/main/{file}"),
    ]
}

pub fn yunzai_market_parts() -> Vec<StoreMarketPart> {
    PARTS
        .iter()
        .map(|(id, file, _)| StoreMarketPart {
            id,
            urls: index_part_urls(file),
        })
        .collect()
}

/// Gitee contents 接口回的是 `{"content": "<base64>", "encoding": "base64", ...}`；别的源就是原文
pub fn decode_part_text(text: &str) -> String {
    let trimmed = text.trim_start();
    if !trimmed.starts_with('{') {
        return text.to_string();
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) else {
        return text.to_string();
    };
    let Some(content) = v.get("content").and_then(|c| c.as_str()) else {
        return text.to_string();
    };
    let compact: String = content.chars().filter(|c| !c.is_whitespace()).collect();
    base64::engine::general_purpose::STANDARD
        .decode(compact)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_else(|_| text.to_string())
}

/// 单元格里的 `[文字](地址)`；文字里可以有括号（`[WeGame-plugin)](…)` 这种手滑也认）
pub fn md_links(cell: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = cell;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let Some(mid) = after.find("](") else { break };
        let text = &after[..mid];
        let url_part = &after[mid + 2..];
        let Some(close) = url_part.find(')') else { break };
        out.push((text.trim().to_string(), url_part[..close].trim().to_string()));
        rest = &url_part[close + 1..];
    }
    out
}

/// 去掉链接语法和粗体 / 删除线 / 换行标签，剩下读得通的一句
pub fn md_plain(cell: &str) -> String {
    let mut s = String::with_capacity(cell.len());
    let mut rest = cell;
    loop {
        match (rest.find('['), rest.find("](")) {
            (Some(open), Some(mid)) if open < mid => {
                let url_start = mid + 2;
                if let Some(close) = rest[url_start..].find(')') {
                    s.push_str(&rest[..open]);
                    s.push_str(&rest[open + 1..mid]);
                    rest = &rest[url_start + close + 1..];
                    continue;
                }
                s.push_str(rest);
                break;
            }
            _ => {
                s.push_str(rest);
                break;
            }
        }
    }
    let s = s
        .replace("**", "")
        .replace("~~", "")
        .replace("<br>", " ")
        .replace("<br/>", " ")
        .replace("&nbsp;", " ")
        .replace('`', "");
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 修掉索引里常见的几种坏链接：全角冒号、首尾空白、写给 Gitee 页面看的相对路径
pub fn normalize_url(raw: &str) -> Option<String> {
    let mut u = raw.trim().replace('：', ":");
    if u.is_empty() {
        return None;
    }
    if u.starts_with("../") || u.starts_with("./") {
        let mut rest = u.as_str();
        while let Some(r) = rest.strip_prefix("../").or_else(|| rest.strip_prefix("./")) {
            rest = r;
        }
        u = format!("https://gitee.com/{rest}");
    }
    if !(u.starts_with("https://") || u.starts_with("http://")) {
        return None;
    }
    Some(u)
}

fn split_url(url: &str) -> Option<(&str, &str, Vec<&str>)> {
    let (scheme, rest) = url.split_once("://")?;
    let rest = rest.split(['#', '?']).next().unwrap_or(rest);
    let mut parts = rest.split('/');
    let host = parts.next()?;
    let segs: Vec<&str> = parts.filter(|s| !s.is_empty()).collect();
    Some((scheme, host, segs))
}

/// 主页链接 → 能 clone 的仓库根 `https://host/owner/repo`（去掉 .git、/tree/xx、锚点）
pub fn repo_root(url: &str) -> Option<String> {
    let (scheme, host, segs) = split_url(url)?;
    if segs.len() < 2 || host.starts_with("raw.") || host.contains("qq.com") {
        return None;
    }
    let repo = segs[1].trim_end_matches(".git");
    if repo.is_empty() {
        return None;
    }
    Some(format!("{scheme}://{host}/{}/{repo}", segs[0]))
}

/// 仓库根 → 插件目录名（就是仓库名）
pub fn repo_dir_name(root: &str) -> Option<String> {
    let name = root.rsplit('/').next()?.trim_end_matches(".git").to_string();
    (!name.is_empty() && !name.contains("..")).then_some(name)
}

pub fn repo_kind(url: &str) -> &'static str {
    match split_url(url).map(|(_, h, _)| h) {
        Some(h) if h.ends_with("github.com") => "github",
        Some(h) if h.ends_with("gitee.com") => "gitee",
        Some(h) if h.ends_with("gitcode.com") => "gitcode",
        Some(h) if h.ends_with("gitlab.com") => "gitlab",
        _ => "git",
    }
}

fn is_git_host(url: &str) -> bool {
    repo_kind(url) != "git"
}

/// 单 JS 的「源码」链接 → 直接能下的 raw 地址；认不出、或者指的不是 .js 文件就是 None
pub fn js_raw_url(url: &str) -> Option<String> {
    let url = normalize_url(url)?;
    let (_, host, segs) = split_url(&url)?;
    let tail_js = segs
        .last()
        .is_some_and(|s| percent_decode_lossy(s).to_ascii_lowercase().ends_with(".js"));
    if !tail_js {
        return None;
    }
    let path_after = |n: usize| segs[n..].join("/");
    let raw = match host {
        "raw.githubusercontent.com" | "raw.gitcode.com" => url.split(['#', '?']).next()?.to_string(),
        "github.com" if segs.len() > 4 && segs[2] == "blob" => format!(
            "https://raw.githubusercontent.com/{}/{}/{}",
            segs[0],
            segs[1],
            path_after(3)
        ),
        // github.com/…/raw/… 会 302 到 raw.githubusercontent.com；远端用 curl / wget 下载时不一定跟跳转，直接写目标
        "github.com" if segs.len() > 4 && segs[2] == "raw" => format!(
            "https://raw.githubusercontent.com/{}/{}/{}",
            segs[0],
            segs[1],
            path_after(3)
        ),
        "gitee.com" if segs.len() > 4 && (segs[2] == "blob" || segs[2] == "raw") => format!(
            "https://gitee.com/{}/{}/raw/{}",
            segs[0],
            segs[1],
            path_after(3)
        ),
        "gitcode.com" if segs.len() > 4 && (segs[2] == "blob" || segs[2] == "raw") => format!(
            "https://raw.gitcode.com/{}/{}/raw/{}",
            segs[0],
            segs[1],
            path_after(3)
        ),
        _ => return None,
    };
    Some(raw)
}

/// 表格里的一行拆成单元格；表头、分隔行、非表格行给 None
pub fn table_cells(line: &str) -> Option<Vec<String>> {
    let t = line.trim();
    // 单 JS 那张表里有一行漏了开头的 |
    if !(t.starts_with('|') || (t.starts_with('[') && t.contains(" | "))) {
        return None;
    }
    let inner = t.strip_prefix('|').unwrap_or(t);
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    let cells: Vec<String> = inner.split('|').map(|c| c.trim().to_string()).collect();
    if cells.len() < 3 {
        return None;
    }
    let first = &cells[0];
    if first.chars().all(|c| c == '-' || c == ':' || c == ' ') {
        return None;
    }
    if first.contains("名称") && cells.iter().any(|c| c.contains("作者")) {
        return None;
    }
    Some(cells)
}

/// 插件自己 README 要求的目录名和仓库名不一样的几个（取自上游 `plugins/other/install.js` 的安装表）
const DIR_NAME_OVERRIDES: &[(&str, &str)] = &[
    ("guoba-yunzai/guoba-plugin", "Guoba-Plugin"),
    ("nwflower/atlas", "Atlas"),
    ("snowtafir/xianxin-plugin", "trss-xianxin-plugin"),
    ("timerainstarsky/yunzai-genshin", "genshin"),
    ("lovely02y/secluded-adapter", "Secluded-Plugin"),
];

/// 上游安装表里另给的镜像：gitee 上的喵喵要登录了，TRSS 在 gitcode 放了一份
const EXTRA_MIRRORS: &[(&str, &str)] = &[
    ("miao-plugin", "https://gitcode.com/TimeRainStarSky/miao-plugin"),
    ("xiaoyao-cvs-plugin", "https://gitcode.com/TimeRainStarSky/xiaoyao-cvs-plugin"),
];

fn owner_repo_key(root: &str) -> Option<String> {
    let (_, _, segs) = split_url(root)?;
    (segs.len() >= 2).then(|| format!("{}/{}", segs[0], segs[1].trim_end_matches(".git")).to_ascii_lowercase())
}

/// 插件目录名：安装表里点过名的用它的，其余用仓库名
pub fn plugin_dir_for(root: &str) -> Option<String> {
    let key = owner_repo_key(root)?;
    if let Some((_, dir)) = DIR_NAME_OVERRIDES.iter().find(|(k, _)| *k == key) {
        return Some((*dir).to_string());
    }
    repo_dir_name(root)
}

fn author_list(cell: &str) -> Vec<KarinPluginAuthor> {
    let links = md_links(cell);
    if links.is_empty() {
        let name = md_plain(cell).trim_start_matches('@').trim().to_string();
        return if name.is_empty() {
            Vec::new()
        } else {
            vec![KarinPluginAuthor {
                name,
                home: String::new(),
            }]
        };
    }
    links
        .into_iter()
        .map(|(text, url)| KarinPluginAuthor {
            name: text.trim().trim_start_matches('@').trim().to_string(),
            home: normalize_url(&url).unwrap_or_default(),
        })
        .filter(|a| !a.name.is_empty())
        .collect()
}

fn repo(url: String) -> KarinPluginRepo {
    KarinPluginRepo {
        r#type: repo_kind(&url).to_string(),
        url,
        branch: String::new(),
    }
}

fn base_entry(id: String, name: String, tag: &str) -> AppStoreMarketEntry {
    AppStoreMarketEntry {
        resource: AppStoreResource::Plugin,
        id: id.clone(),
        name,
        description: String::new(),
        version: String::new(),
        author: String::new(),
        homepage: String::new(),
        time: String::new(),
        package: id.clone(),
        module_name: id,
        flavor: AppStoreFlavor::Git,
        is_official: tag == "推荐",
        valid: true,
        tags: vec![tag.to_string()],
        supported_adapters: Vec::new(),
        authors: Vec::new(),
        repos: Vec::new(),
        files: Vec::new(),
        allow_build: Vec::new(),
    }
}

/// 目录插件那三张表（和 README 的推荐表）的一行
fn git_entry(cells: &[String], tag: &str) -> Option<AppStoreMarketEntry> {
    let (name, home) = md_links(&cells[0]).into_iter().next()?;
    let home = normalize_url(&home)?;
    let note = cells.get(2).map(String::as_str).unwrap_or_default();
    let root = repo_root(&home).filter(|r| is_git_host(r));
    let id = root
        .as_deref()
        .and_then(plugin_dir_for)
        .unwrap_or_else(|| format!("link:{}", md_plain(&name)));
    let mut entry = base_entry(id, md_plain(&name), tag);
    entry.homepage = home;
    entry.description = md_plain(note);
    entry.authors = author_list(cells.get(1).map(String::as_str).unwrap_or_default());
    entry.author = entry
        .authors
        .iter()
        .map(|a| a.name.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    if let Some(root) = root {
        entry.repos.push(repo(root));
        // 备注里另给的「GitHub 镜像」「Gitee 仓库」也是同一个插件的源
        for (text, url) in md_links(note) {
            let Some(url) = normalize_url(&url) else { continue };
            let Some(mirror) = repo_root(&url).filter(|r| is_git_host(r)) else { continue };
            let looks_mirror = text.contains("镜像") || text.contains("仓库");
            if looks_mirror && !entry.repos.iter().any(|r| r.url == mirror) {
                entry.repos.push(repo(mirror));
            }
        }
    } else {
        entry.valid = false;
    }
    Some(entry)
}

/// 单 JS 表的一行：`| [名称](主页) | [@作者](…) | [查看源码](raw) | 备注 |`
fn js_entry(cells: &[String]) -> Option<AppStoreMarketEntry> {
    if cells.len() < 4 {
        return None;
    }
    let (name, home) = md_links(&cells[0]).into_iter().next()?;
    let files: Vec<KarinPluginAppFile> = md_links(&cells[2])
        .into_iter()
        .filter_map(|(_, url)| js_raw_url(&url))
        .map(|url| KarinPluginAppFile {
            name: crate::karin::plugin::app_file_basename(&url).unwrap_or_default(),
            url,
            description: String::new(),
        })
        .filter(|f| !f.name.is_empty())
        .collect();
    // 同一个文件的几个源才合在一条里；给了两个不同文件名的只认第一个
    let first = files.first().map(|f| f.name.clone());
    let files: Vec<KarinPluginAppFile> = files
        .into_iter()
        .filter(|f| Some(&f.name) == first.as_ref())
        .collect();
    let id = first.clone().unwrap_or_else(|| format!("link:{}", md_plain(&name)));
    let mut entry = base_entry(id, md_plain(&name), "单 JS");
    entry.flavor = AppStoreFlavor::App;
    entry.homepage = normalize_url(&home).unwrap_or_default();
    entry.description = md_plain(&cells[3]);
    entry.authors = author_list(&cells[1]);
    entry.author = entry
        .authors
        .iter()
        .map(|a| a.name.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    entry.valid = first.is_some();
    entry.files = files;
    Some(entry)
}

/// README 里只取「推荐插件」那一节的表（前面「云崽框架」那张是本体，不是插件）
fn recommended_section(text: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for line in text.lines() {
        if line.starts_with("## ") {
            inside = line.contains("推荐插件");
            continue;
        }
        if inside {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// 装云崽之后几乎都要装的两个：TRSS 版原神基础（喵喵依赖它）和 TRSS 插件。索引里没有，按上游安装表补上
fn builtin_entries() -> Vec<AppStoreMarketEntry> {
    let mut genshin = base_entry("genshin".into(), "原神基础 (genshin)".into(), "推荐");
    genshin.description = "TRSS 版原神基础功能，装喵喵插件前先装它".into();
    genshin.author = "时雨🌌星空".into();
    genshin.homepage = "https://github.com/TimeRainStarSky/Yunzai-genshin".into();
    genshin.repos = vec![
        repo("https://gitee.com/TimeRainStarSky/Yunzai-genshin".into()),
        repo("https://github.com/TimeRainStarSky/Yunzai-genshin".into()),
    ];
    let mut trss = base_entry("TRSS-Plugin".into(), "TRSS 插件 (TRSS-Plugin)".into(), "推荐");
    trss.description = "TRSS 自带的工具箱：远程命令、文件操作、语音合成等".into();
    trss.author = "时雨🌌星空".into();
    trss.homepage = "https://github.com/TimeRainStarSky/TRSS-Plugin".into();
    trss.repos = vec![
        repo("https://gitee.com/TimeRainStarSky/TRSS-Plugin".into()),
        repo("https://github.com/TimeRainStarSky/TRSS-Plugin".into()),
    ];
    vec![genshin, trss]
}

/// 几份原文 → 商店条目。先推荐（内置 + README），再按分类；同一个插件在多张表里出现只留一条、标签合并
pub fn parse_index(parts: &[StoreMarketText]) -> Vec<AppStoreMarketEntry> {
    let mut out: Vec<AppStoreMarketEntry> = builtin_entries();
    for part in parts {
        let Some((_, _, tag)) = PARTS.iter().find(|(id, _, _)| *id == part.id) else {
            continue;
        };
        let text = decode_part_text(&part.text);
        let body = if part.id == "readme" {
            recommended_section(&text)
        } else {
            text
        };
        for line in body.lines() {
            let Some(cells) = table_cells(line) else { continue };
            let entry = if part.id == "js" {
                js_entry(&cells)
            } else {
                git_entry(&cells, tag)
            };
            let Some(entry) = entry else { continue };
            match out.iter_mut().find(|e| e.id == entry.id) {
                Some(existing) => {
                    for t in entry.tags {
                        if !existing.tags.contains(&t) {
                            existing.tags.push(t);
                        }
                    }
                    for r in entry.repos {
                        if !existing.repos.iter().any(|x| x.url == r.url) {
                            existing.repos.push(r);
                        }
                    }
                    if existing.description.is_empty() {
                        existing.description = entry.description;
                    }
                }
                None => out.push(entry),
            }
        }
    }
    for entry in &mut out {
        if let Some((_, mirror)) = EXTRA_MIRRORS.iter().find(|(id, _)| *id == entry.id) {
            if !entry.repos.iter().any(|r| r.url == *mirror) {
                entry.repos.push(repo((*mirror).to_string()));
            }
        }
    }
    out
}

// ---- 主机上的装卸 ----

fn host_err(e: HostError) -> AppFrameworkError {
    AppFrameworkError::Host(e.to_string())
}

fn emit(log: Option<&PluginLogSink>, line: impl Into<String>) {
    if let Some(sink) = log {
        sink(line.into());
    }
}

/// 插件名进路径之前挡一道：不许分隔符、`..`、控制字符
pub fn reject_unsafe_name(name: &str) -> Result<(), AppFrameworkError> {
    let bad = name.is_empty()
        || name == "."
        || name.contains("..")
        || name.contains('/')
        || name.contains('\\')
        || name.chars().any(char::is_control);
    if bad {
        return Err(AppFrameworkError::Validation(format!("插件名不合法: {name}")));
    }
    Ok(())
}

pub fn plugin_dir(instance: &AppInstance, name: &str) -> HostPath {
    HostPath::from_posix(&instance.install_dir)
        .join(YUNZAI_PLUGINS_DIR)
        .join(name)
}

pub fn js_path(instance: &AppInstance, basename: &str) -> HostPath {
    HostPath::from_posix(&instance.install_dir)
        .join(YUNZAI_JS_PLUGIN_DIR)
        .join(basename)
}

/// 装实例时用的 git；标记没有就看 PATH
pub async fn instance_git(host: &dyn Host, instance: &AppInstance) -> Result<GitTool, AppFrameworkError> {
    let marker = HostPath::from_posix(&instance.install_dir).join(GIT_MARKER_FILE);
    let managed = match host.read_file(&marker).await {
        Ok(bytes) => {
            let line = String::from_utf8_lossy(&bytes).lines().next().unwrap_or("").trim().to_string();
            (!line.is_empty()).then(|| HostPath::from_posix(line))
        }
        Err(_) => None,
    };
    GitTool::resolve(host, managed.as_ref())
        .await
        .map_err(AppFrameworkError::Validation)
}

async fn run_logged(
    host: &dyn Host,
    cmd: HostCommand,
    what: &str,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    let sink = log.cloned();
    let out = host
        .run_streaming(
            cmd,
            Box::new(move |_src, line| {
                let t = line.trim_end();
                if !t.is_empty() {
                    if let Some(s) = &sink {
                        s(t.to_string());
                    }
                }
            }),
        )
        .await
        .map_err(host_err)?;
    if out.success() {
        Ok(())
    } else {
        Err(AppFrameworkError::Runtime(format!(
            "{what} 失败（exit={:?}）：{}",
            out.exit_code,
            last_line(&out.stderr)
        )))
    }
}

/// 插件带 package.json 时在实例根跑一次 pnpm install（`plugins/**` 是 workspace，上游 #安装插件 也是这样）
async fn pnpm_install_if_needed(
    host: &dyn Host,
    instance: &AppInstance,
    dir: &HostPath,
    git: &GitTool,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    if !host.exists(&dir.join("package.json")).await.map_err(host_err)? {
        return Ok(());
    }
    let root = HostPath::from_posix(&instance.install_dir);
    let mut preferred = Vec::new();
    if let Some(p) = read_node_marker(host, &root).await {
        preferred.push(p);
    }
    let tc = resolve_node_toolchain(host, &preferred)
        .await
        .map_err(|e| AppFrameworkError::Runtime(e.to_string()))?;
    let mut prefix = path_prefix(&tc, &root, host.os());
    if let Some(d) = &git.dir {
        prefix.push(d.render_for(host.os()));
    }
    let path_env = match host.locality() {
        ncd_host::Locality::Local => local_path_env(&prefix, host.os()),
        ncd_host::Locality::Remote => {
            prefix.extend(["/usr/local/bin", "/usr/bin", "/bin"].map(String::from));
            prefix.join(":")
        }
    };
    emit(log, "pnpm install（装插件依赖）");
    let cmd = pnpm_command(&tc, &root, host.os(), &["install", "--no-frozen-lockfile"])
        .env("CI", "true")
        .env("PUPPETEER_SKIP_DOWNLOAD", "true")
        .env("PATH", path_env)
        .timeout(PLUGIN_CMD_TIMEOUT);
    run_logged(host, cmd, "pnpm install", log).await
}

/// 目录插件：按条目给的源逐个 clone（GitHub 的再补几个加速前缀），成功一个就停
pub async fn install_git_plugin(
    host: &dyn Host,
    instance: &AppInstance,
    entry: &AppStoreMarketEntry,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    reject_unsafe_name(&entry.id)?;
    if !entry.valid || entry.repos.is_empty() {
        return Err(AppFrameworkError::Validation(format!(
            "{} 在索引里没有能 clone 的仓库地址，去它的主页手动装",
            entry.name
        )));
    }
    let git = instance_git(host, instance).await?;
    let dest = plugin_dir(instance, &entry.id);
    if host.exists(&dest).await.map_err(host_err)? {
        return Err(AppFrameworkError::Validation(format!("plugins/{} 已经存在", entry.id)));
    }
    host.create_dir_all(&HostPath::from_posix(&instance.install_dir).join(YUNZAI_PLUGINS_DIR))
        .await
        .map_err(host_err)?;
    let urls: Vec<String> = entry.repos.iter().map(|r| r.url.clone()).collect();
    let dest_arg = dest.render_for(host.os());
    let mut last_err = None;
    for url in clone_candidates(&urls) {
        emit(log, format!("git clone --depth 1 {url}"));
        let cmd = git
            .command(["clone", "--depth", "1", "--single-branch", url.as_str(), dest_arg.as_str()])
            .working_dir(HostPath::from_posix(&instance.install_dir))
            .timeout(GIT_LONG_TIMEOUT);
        match run_logged(host, cmd, "git clone", log).await {
            Ok(()) => {
                last_err = None;
                break;
            }
            Err(e) => {
                emit(log, format!("这个源不行：{e}"));
                let _ = host.remove_dir_all(&dest).await;
                last_err = Some(e);
            }
        }
    }
    if let Some(e) = last_err {
        return Err(e);
    }
    pnpm_install_if_needed(host, instance, &dest, &git, log).await
}

/// `git pull --ff-only`：插件目录里常有用户改过的受版本管理的文件，不替用户 reset
pub async fn update_git_plugin(
    host: &dyn Host,
    instance: &AppInstance,
    name: &str,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    reject_unsafe_name(name)?;
    let git = instance_git(host, instance).await?;
    let dest = plugin_dir(instance, name);
    if !host.exists(&dest.join(".git")).await.map_err(host_err)? {
        return Err(AppFrameworkError::Validation(format!(
            "plugins/{name} 不是 git 仓库，没法更新；卸载后重装"
        )));
    }
    let dir = dest.render_for(host.os());
    emit(log, format!("git pull {name}"));
    let cmd = git
        .command(["-C", dir.as_str(), "pull", "--ff-only"])
        .timeout(GIT_LONG_TIMEOUT);
    run_logged(host, cmd, "git pull", log).await.map_err(|e| {
        AppFrameworkError::Runtime(format!(
            "{e}。多半是插件目录里有改过的文件或历史对不上，可以在终端进 plugins/{name} 处理，或卸载重装"
        ))
    })?;
    pnpm_install_if_needed(host, instance, &dest, &git, log).await
}

pub async fn remove_git_plugin(
    host: &dyn Host,
    instance: &AppInstance,
    name: &str,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    reject_unsafe_name(name)?;
    if YUNZAI_BUILTIN_PLUGIN_DIRS.contains(&name) {
        return Err(AppFrameworkError::Validation(format!("plugins/{name} 是云崽自带的，不能卸载")));
    }
    let dest = plugin_dir(instance, name);
    emit(log, format!("删除 plugins/{name}"));
    if host.exists(&dest).await.map_err(host_err)? {
        host.remove_dir_all(&dest).await.map_err(host_err)?;
    }
    Ok(())
}

/// 单 JS：启用的和停用的都删掉
pub async fn remove_js_plugin(
    host: &dyn Host,
    instance: &AppInstance,
    basename: &str,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    reject_unsafe_name(basename)?;
    emit(log, format!("删除 plugins/example/{basename}"));
    for path in [
        js_path(instance, basename),
        js_path(instance, &format!("{basename}{DISABLED_SUFFIX}")),
    ] {
        if host.exists(&path).await.map_err(host_err)? {
            host.remove_file(&path).await.map_err(host_err)?;
        }
    }
    Ok(())
}

/// 单 JS 启停：`x.js` ↔ `x.js.disabled`（example 目录热加载，改名后不用重启）
pub async fn set_js_enabled(
    host: &dyn Host,
    instance: &AppInstance,
    basename: &str,
    enabled: bool,
) -> Result<(), AppFrameworkError> {
    reject_unsafe_name(basename)?;
    let on = js_path(instance, basename);
    let off = js_path(instance, &format!("{basename}{DISABLED_SUFFIX}"));
    let (from, to) = if enabled { (off, on) } else { (on, off) };
    if !host.exists(&from).await.map_err(host_err)? {
        // 已经是想要的状态
        if host.exists(&to).await.map_err(host_err)? {
            return Ok(());
        }
        return Err(AppFrameworkError::Validation(format!("没找到 {basename}")));
    }
    host.rename(&from, &to).await.map_err(host_err)
}

async fn list_dir_or_empty(
    host: &dyn Host,
    path: &HostPath,
) -> Result<Vec<ncd_host::DirEntry>, AppFrameworkError> {
    if !host.exists(path).await.map_err(host_err)? {
        return Ok(Vec::new());
    }
    host.list_dir(path).await.map_err(host_err)
}

async fn package_version(host: &dyn Host, dir: &HostPath) -> Option<String> {
    let bytes = host.read_file(&dir.join("package.json")).await.ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    v.get("version")?.as_str().map(str::to_string).filter(|s| !s.is_empty())
}

/// 已装：plugins 下除自带四个目录外的每个目录（目录插件），加 plugins/example 下的 .js（单 JS）
pub async fn list_installed(
    host: &dyn Host,
    instance: &AppInstance,
) -> Result<Vec<AppStoreInstalled>, AppFrameworkError> {
    let root = HostPath::from_posix(&instance.install_dir);
    let mut out = Vec::new();
    for entry in list_dir_or_empty(host, &root.join(YUNZAI_PLUGINS_DIR)).await? {
        if !entry.is_dir || entry.name.starts_with('.') {
            continue;
        }
        if YUNZAI_BUILTIN_PLUGIN_DIRS.contains(&entry.name.as_str()) {
            continue;
        }
        let dir = root.join(YUNZAI_PLUGINS_DIR).join(&entry.name);
        out.push(AppStoreInstalled {
            id: entry.name.clone(),
            name: entry.name.clone(),
            resource: AppStoreResource::Plugin,
            flavor: AppStoreFlavor::Git,
            version: package_version(host, &dir).await,
            enabled: true,
            package: entry.name,
            locked: false,
        });
    }
    for entry in list_dir_or_empty(host, &root.join(YUNZAI_JS_PLUGIN_DIR)).await? {
        if entry.is_dir {
            continue;
        }
        let (id, enabled) = match entry.name.strip_suffix(DISABLED_SUFFIX) {
            Some(base) if base.ends_with(".js") => (base.to_string(), false),
            None if entry.name.ends_with(".js") => (entry.name.clone(), true),
            _ => continue,
        };
        out.push(AppStoreInstalled {
            id: id.clone(),
            name: id.clone(),
            resource: AppStoreResource::Plugin,
            flavor: AppStoreFlavor::App,
            version: None,
            enabled,
            package: id,
            locked: false,
        });
    }
    Ok(out)
}

pub async fn confirm_on_disk(
    host: &dyn Host,
    instance: &AppInstance,
    entry: &AppStoreMarketEntry,
) -> Result<(), AppFrameworkError> {
    let path = match entry.flavor {
        AppStoreFlavor::App => js_path(instance, &entry.id),
        _ => plugin_dir(instance, &entry.id),
    };
    if host.exists(&path).await.map_err(host_err)? {
        Ok(())
    } else {
        Err(AppFrameworkError::Runtime(format!("命令已结束，但没找到 {}", entry.id)))
    }
}

// ---- 插件配置文件 ----

const PLUGIN_DOC_PREFIX: &str = "plugin:";
/// 这些目录放的是插件自带的默认模板，改了也会被插件更新覆盖
const TEMPLATE_DIRS: &[&str] = &["default_config", "defSet", "default", "system"];

pub fn plugin_doc_id(plugin: &str, rel: &str) -> String {
    format!("{PLUGIN_DOC_PREFIX}{plugin}:{rel}")
}

pub fn parse_plugin_doc_id(doc_id: &str) -> Option<(&str, &str)> {
    doc_id.strip_prefix(PLUGIN_DOC_PREFIX)?.split_once(':')
}

fn doc_format(rel: &str) -> Option<AppConfigFormat> {
    let lower = rel.to_ascii_lowercase();
    if lower.ends_with(".yaml") || lower.ends_with(".yml") {
        Some(AppConfigFormat::Yaml)
    } else if lower.ends_with(".json") {
        Some(AppConfigFormat::Json)
    } else {
        None
    }
}

/// 插件配置文档；rel 相对插件目录，不许跳出去
pub fn plugin_config_document(plugin: &str, rel: &str) -> Result<AppConfigDocument, AppFrameworkError> {
    reject_unsafe_name(plugin)?;
    if rel.is_empty() || rel.contains("..") || rel.contains('\\') || rel.starts_with('/') {
        return Err(AppFrameworkError::Validation(format!("配置路径不合法: {rel}")));
    }
    let format = doc_format(rel)
        .ok_or_else(|| AppFrameworkError::Validation(format!("不是 YAML / JSON 文件: {rel}")))?;
    Ok(AppConfigDocument {
        id: plugin_doc_id(plugin, rel),
        label: rel.to_string(),
        rel_path: format!("{YUNZAI_PLUGINS_DIR}/{plugin}/{rel}"),
        format,
        hot_reload: true,
    })
}

/// 插件 `config/` 下两层以内的 YAML / JSON（跳过默认模板目录）
pub async fn list_plugin_config_docs(
    host: &dyn Host,
    instance: &AppInstance,
    plugin: &str,
) -> Result<Vec<AppConfigDocument>, AppFrameworkError> {
    reject_unsafe_name(plugin)?;
    let config_dir = plugin_dir(instance, plugin).join("config");
    let mut rels = Vec::new();
    for entry in list_dir_or_empty(host, &config_dir).await? {
        if entry.is_dir {
            if TEMPLATE_DIRS.contains(&entry.name.as_str()) {
                continue;
            }
            for inner in list_dir_or_empty(host, &config_dir.join(&entry.name)).await? {
                if !inner.is_dir && doc_format(&inner.name).is_some() {
                    rels.push(format!("config/{}/{}", entry.name, inner.name));
                }
            }
        } else if doc_format(&entry.name).is_some() {
            rels.push(format!("config/{}", entry.name));
        }
    }
    rels.sort();
    rels.iter().map(|rel| plugin_config_document(plugin, rel)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // 从 2026-09-29 的索引原文里摘的行，带着原样的毛病（相对链接、全角冒号、漏开头的 |、stray 括号）
    const FUNCTION: &str = "## 功能类插件索引\n\n<!-- [GUOBA:FUNCTION_PLUGIN:BEGIN] 锅巴插件访问标记，请勿移动 -->\n\n<!-- 请在表首添加新行 -->\n| 名称  |  作者  | 备注  |\n|-------| ----- |------ |\n| [表情包插件 (meme-plugin)](https://gitee.com/longhengmu/meme-plugin) | [@龙横木](https://gitee.com/longhengmu) | 给云崽装上 900 多个表情包：中文指令出图。[GitHub 镜像](https://github.com/cchanlan/meme-plugin) |\n| [宝塔插件(BTPanel-Plugin)](../../../../../yll14/btpanel-plugin) | [@yll14](../../../../../yll14) | 宝塔面板运维插件 |\n| [daily-Plugin](https://gitee.com/yll14/daily-plugin) | [@桉南](https：//gitee.com/yll14) | xingluo-plugin（星落插件）复活版 |\n| [agents-plugin](https://github.com/yunhai89/agents-plugin.git) | [@yunhai89](https://github.com/yunhai89) | 适用于 TRSS-Yunzai 的 AI Agent 插件，**高危**操作主人审批。|\n| [Secluded-Adapter](https://github.com/Lovely02Y/Secluded-Adapter) | [@Lovely02Y](https://github.com/Lovely02Y) [@Senior Horikawa](https://github.com/SeniorHorikawa) | 一个普通的协议适配器 |\n| [wordle-plugin](https://gitee.com/qingyingxbot/wordle-plugin) | [@QingYingX](https://gitee.com/QingYingX) | 游戏插件 |\n| [wordle-plugin](https://github.com/Pimeng/wordle-plugin) | [@Pimeng](https://github.com/Pimeng) | Wordle猜词 |\n| [锅巴插件 Next (guoba-plugin-next)](https://gitee.com/longhengmu/guoba-plugin-next) | [@龙横木](https://gitee.com/longhengmu) | 管理面板 |\n";
    const JS: &str = "| 名称  |  作者  | 源码 | 备注  |\n| --- | --- | ---- | ------- |\n| [QQBot官机兑换码复制按钮](https://github.com/shiomon/codes)|  [@shiomon](https://github.com/shiomon) | [查看源码](https://raw.githubusercontent.com/shiomon/codes/main/%E5%85%91%E6%8D%A2%E7%A0%81.js)  | 发送各种游戏的兑换码 |\n| [喵言喵语](https://gitee.com/VanillaNahida/yunzai-js-plugin#x) | [@香草味的纳西妲喵](https://github.com/VanillaNahida) | [Github](https://raw.githubusercontent.com/VanillaNahida/yunzai-js-plugin/refs/heads/main/%E5%96%B5%E8%A8%80%E5%96%B5%E8%AF%AD.js) & [Gitee](https://gitee.com/VanillaNahida/yunzai-js-plugin/raw/main/%E5%96%B5%E8%A8%80%E5%96%B5%E8%AF%AD.js) | 快捷发送卡拉彼丘的喵语 |\n| [消息追加文本](https://github.com/herijian1/message-append-text)|  [@何日见](https://github.com/herijian1) | [查看源码](https://github.com/herijian1/message-append-text/消息追加文本.js)  | 拦截机器人发的消息 |\n| [米游材料背包查询](https://github.com/devil233-ui/HoyoMaterialPack) | [@devil](https://github.com/devil233-ui) | [查看源码](https://github.com/devil233-ui/HoyoMaterialPack/raw/refs/heads/master/plugins/example/%E7%B1%B3%E6%B8%B8.js) |获取米游材料背包\n| [Help_Lite](https://gitcode.com/T060925ZX/help-plugin/)|  [@Jiaozi](https://github.com/T060925ZX) | [查看源码](https://gitcode.com/T060925ZX/help-plugin/blob/main/Help_Lite.js)  | 简约美观的菜单 |\n[卢浮宫插件](https://gitee.com/aozorayui/JS-Plugin) | [@青空由依](https://gitee.com/aozorayui) | [Github](https://github.com/AozoraYui/JS-Plugin/blob/main/one-last-image.js)&[Gitee](https://gitee.com/aozorayui/JS-Plugin/blob/master/one-last-image.js) | 卢浮宫风格 |\n| [完美许愿器](https://github.com/Temmie0125/Yunzai-JS-Plugin#x) | [@Temmie](https://github.com/Temmie0125) | [查看源码](https://github.com/Temmie0125/Yunzai-JS-Plugin.git) | 因果律模拟 |\n";
    const README: &str = "## 🤖 云崽框架\n\n| 名称 | 作者 | GitHub | Gitee | 备注  | 推荐使用优先级 |\n|------| ---- | ------ | ----- | ----- | ----- |\n| TRSS-Yunzai | [@时雨🌌星空](../../../../TimeRainStarSky) | [☞GitHub](https://github.com/TimeRainStarSky/Yunzai) | [☞Gitee](https://gitee.com/TimeRainStarSky/Yunzai) | Yunzai 应用端 | ■■■■■ |\n\n## ⭐️ 推荐插件\n\n| 名称  |  作者  | 备注  |\n|-------| ----- |------ |\n| [喵喵插件 (miao-plugin)](https://github.com/yoimiya-kokomi/miao-plugin) | [@喵喵](https://gitee.com/yoimiya-kokomi)| Miao-Plugin是一个Yunzai-Bot的升级插件 |\n| [锅巴插件 (guoba-plugin)](https://gitee.com/guoba-yunzai/guoba-plugin) | [@zolay-poi](https://github.com/zolay-poi) | 网页端后台管理界面 |\n\n## 🛠️ 功能类插件\n\n[>>>点击此处跳转<<<](./Function-Plugin.md)\n";

    fn part(id: &'static str, text: &str) -> StoreMarketText {
        StoreMarketText {
            id,
            text: text.to_string(),
        }
    }

    fn by_id<'a>(list: &'a [AppStoreMarketEntry], id: &str) -> &'a AppStoreMarketEntry {
        list.iter().find(|e| e.id == id).unwrap_or_else(|| panic!("缺 {id}"))
    }

    #[test]
    fn function_rows_become_git_entries_with_clean_links() {
        let list = parse_index(&[part("function", FUNCTION)]);
        let meme = by_id(&list, "meme-plugin");
        assert_eq!(meme.flavor, AppStoreFlavor::Git);
        assert_eq!(meme.tags, vec!["功能".to_string()]);
        assert_eq!(
            meme.repos.iter().map(|r| r.url.as_str()).collect::<Vec<_>>(),
            vec!["https://gitee.com/longhengmu/meme-plugin", "https://github.com/cchanlan/meme-plugin"]
        );
        assert_eq!(meme.description, "给云崽装上 900 多个表情包：中文指令出图。GitHub 镜像");
        assert_eq!(meme.author, "龙横木");

        let bt = by_id(&list, "btpanel-plugin");
        assert_eq!(bt.homepage, "https://gitee.com/yll14/btpanel-plugin");
        assert_eq!(bt.repos[0].url, "https://gitee.com/yll14/btpanel-plugin");
        // 目录名取仓库名，不取表里的显示名
        assert_eq!(by_id(&list, "daily-plugin").authors[0].home, "https://gitee.com/yll14");
        let agents = by_id(&list, "agents-plugin");
        assert_eq!(agents.repos[0].url, "https://github.com/yunhai89/agents-plugin");
        assert!(agents.description.contains("高危操作"));
        // 安装表点名的目录名
        let secluded = by_id(&list, "Secluded-Plugin");
        assert_eq!(secluded.author, "Lovely02Y Senior Horikawa");
        // 同名的两条只留前面那条（索引按新到旧排）
        let wordle: Vec<_> = list.iter().filter(|e| e.id == "wordle-plugin").collect();
        assert_eq!(wordle.len(), 1);
        assert_eq!(wordle[0].repos[0].url, "https://gitee.com/qingyingxbot/wordle-plugin");
    }

    #[test]
    fn js_rows_resolve_raw_urls_and_mark_uninstallable() {
        let list = parse_index(&[part("js", JS)]);
        let code = by_id(&list, "兑换码.js");
        assert_eq!(code.flavor, AppStoreFlavor::App);
        assert_eq!(code.files.len(), 1);
        assert_eq!(code.files[0].name, "兑换码.js");
        let miao = by_id(&list, "喵言喵语.js");
        assert_eq!(miao.files.len(), 2, "GitHub 和 Gitee 两个源都留着，下载时依次试");
        assert!(miao.files[1].url.starts_with("https://gitee.com/VanillaNahida/yunzai-js-plugin/raw/main/"));
        let bag = by_id(&list, "米游.js");
        assert_eq!(
            bag.files[0].url,
            "https://raw.githubusercontent.com/devil233-ui/HoyoMaterialPack/refs/heads/master/plugins/example/%E7%B1%B3%E6%B8%B8.js"
        );
        assert_eq!(
            by_id(&list, "Help_Lite.js").files[0].url,
            "https://raw.gitcode.com/T060925ZX/help-plugin/raw/main/Help_Lite.js"
        );
        let louvre = by_id(&list, "one-last-image.js");
        assert_eq!(louvre.files[0].url, "https://raw.githubusercontent.com/AozoraYui/JS-Plugin/main/one-last-image.js");
        assert_eq!(louvre.files[1].url, "https://gitee.com/aozorayui/JS-Plugin/raw/master/one-last-image.js");
        // 源码链接不是一个 .js 文件：留在商店里能点主页，但标成装不了
        let broken = by_id(&list, "link:消息追加文本");
        assert!(!broken.valid);
        assert!(!by_id(&list, "link:完美许愿器").valid);
    }

    #[test]
    fn readme_contributes_only_recommended_section_and_builtins_come_first() {
        let list = parse_index(&[part("readme", README), part("function", FUNCTION)]);
        assert_eq!(list[0].id, "genshin");
        assert_eq!(list[1].id, "TRSS-Plugin");
        assert!(list.iter().all(|e| e.id != "Yunzai"), "框架表不是插件");
        let miao = by_id(&list, "miao-plugin");
        assert!(miao.is_official);
        assert!(
            miao.repos.iter().any(|r| r.url == "https://gitcode.com/TimeRainStarSky/miao-plugin"),
            "补上 TRSS 的 gitcode 镜像"
        );
        assert_eq!(by_id(&list, "Guoba-Plugin").tags, vec!["推荐".to_string()]);
    }

    #[test]
    fn gitee_api_payload_is_decoded() {
        let b64 = base64::engine::general_purpose::STANDARD.encode(FUNCTION.as_bytes());
        // Gitee 回的 base64 每 60 个字符一个换行
        let wrapped: String = b64
            .as_bytes()
            .chunks(60)
            .map(|c| std::str::from_utf8(c).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        let json = serde_json::json!({ "type": "file", "encoding": "base64", "content": wrapped }).to_string();
        assert_eq!(decode_part_text(&json), FUNCTION);
        assert_eq!(decode_part_text("| a | b | c |"), "| a | b | c |");
    }

    #[test]
    fn repo_roots_and_dir_names() {
        assert_eq!(
            repo_root("https://gitee.com/wind-trace-typ/sys_status/tree/master").as_deref(),
            Some("https://gitee.com/wind-trace-typ/sys_status")
        );
        assert_eq!(repo_root("https://qm.qq.com/q/QYgwvEH22Q"), None);
        assert_eq!(
            plugin_dir_for("https://gitee.com/guoba-yunzai/guoba-plugin").as_deref(),
            Some("Guoba-Plugin")
        );
        assert_eq!(plugin_dir_for("https://github.com/Nwflower/atlas").as_deref(), Some("Atlas"));
        assert_eq!(repo_kind("https://gitcode.com/a/b"), "gitcode");
    }

    #[test]
    fn index_parts_have_api_raw_and_cdn_sources() {
        let parts = yunzai_market_parts();
        assert_eq!(
            parts.iter().map(|p| p.id).collect::<Vec<_>>(),
            vec!["readme", "function", "game", "wordgame", "js"]
        );
        let js = &parts[4].urls;
        assert!(js[0].starts_with("https://gitee.com/api/v5/repos/yhArcadia/Yunzai-Bot-plugins-index/contents/JS-Plugin.md"));
        assert!(js.iter().any(|u| u.starts_with("https://cdn.jsdelivr.net/gh/")));
    }

    #[test]
    fn unsafe_names_and_doc_ids() {
        assert!(reject_unsafe_name("../x").is_err());
        assert!(reject_unsafe_name("a/b").is_err());
        assert!(reject_unsafe_name("兑换码.js").is_ok());
        let doc = plugin_config_document("miao-plugin", "config/cfg.yaml").unwrap();
        assert_eq!(doc.rel_path, "plugins/miao-plugin/config/cfg.yaml");
        assert_eq!(parse_plugin_doc_id(&doc.id), Some(("miao-plugin", "config/cfg.yaml")));
        assert!(plugin_config_document("miao-plugin", "../../config/config/other.yaml").is_err());
        assert!(plugin_config_document("miao-plugin", "config/a.js").is_err());
    }
}
