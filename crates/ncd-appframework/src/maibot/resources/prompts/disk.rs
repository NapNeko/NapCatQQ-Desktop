//! 停着的时候直接改盘上文件，做法照上游 `config.py` 的提示词接口：
//! - 覆盖 `data/custom_prompts/<语言>/<名>.prompt`，麦麦启动后只读它
//! - 版本 `data/custom_prompts/<语言>/.versions/<名>/<版本号>.prompt`，清单 `manifest.json`
//!   `{"active_version_id", "versions":[{id,label,created_at,modified_at,…}]}`，别的键原样留着
//!
//! 主机接口拿不到文件时间，版本时间用清单里记的。

use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};

use ncd_domain::AppInstance;
use ncd_host::{Host, HostError, HostPath};
use ncd_traits::AppFrameworkError;
use serde_json::{Map, Value};

use super::{
    ACTIVE_LANGUAGE, MaiBotPromptAction, MaiBotPromptCatalog, MaiBotPromptFile, MaiBotPromptInfo,
    MaiBotPromptLanguage, MaiBotPromptVersion, check_prompt,
};

const PROMPTS_DIR: &str = "prompts";
const CUSTOM_DIR: &str = "data/custom_prompts";
const VERSIONS_DIR: &str = ".versions";
const MANIFEST: &str = "manifest.json";
const META: &str = ".meta.toml";
/// 有覆盖、没有版本清单时上游造的那一条
const LEGACY_ID: &str = "legacy-current";
const LEGACY_LABEL: &str = "当前自定义（旧格式）";

fn host_err(e: HostError) -> AppFrameworkError {
    AppFrameworkError::Host(e.to_string())
}

struct Paths {
    default: HostPath,
    custom_dir: HostPath,
    custom: HostPath,
    versions: HostPath,
}

fn root_of(instance: &AppInstance) -> HostPath {
    HostPath::from_posix(&instance.install_dir)
}

fn paths(root: &HostPath, language: &str, name: &str) -> Paths {
    let stem = name.trim_end_matches(".prompt");
    let custom_dir = root.join(CUSTOM_DIR).join(language);
    Paths {
        default: root.join(PROMPTS_DIR).join(language).join(name),
        versions: custom_dir.join(VERSIONS_DIR).join(stem),
        custom: custom_dir.join(name),
        custom_dir,
    }
}

/// 同 Python 的通用换行读法：CRLF / CR 都当 LF
async fn read_text(host: &dyn Host, path: &HostPath) -> Result<Option<String>, AppFrameworkError> {
    if !host.exists(path).await.map_err(host_err)? {
        return Ok(None);
    }
    let bytes = host.read_file(path).await.map_err(host_err)?;
    Ok(Some(
        String::from_utf8_lossy(&bytes)
            .replace("\r\n", "\n")
            .replace('\r', "\n"),
    ))
}

async fn write_text(
    host: &dyn Host,
    dir: &HostPath,
    path: &HostPath,
    text: &str,
) -> Result<(), AppFrameworkError> {
    host.create_dir_all(dir).await.map_err(host_err)?;
    host.write_file(path, text.as_bytes())
        .await
        .map_err(host_err)
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Entry {
    id: String,
    label: String,
    created_at: f64,
    modified_at: f64,
    extra: Map<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct Manifest {
    active: Option<String>,
    entries: Vec<Entry>,
}

/// 读法同上游 `_read_prompt_version_manifest`：没有文件当空；versions 不是数组当空，
/// 不是对象、id 不是字符串的条目丢掉；active 不是字符串当 null
pub(super) fn parse_manifest(raw: &str) -> Option<Manifest> {
    let value: Value = serde_json::from_str(raw).ok()?;
    let obj = value.as_object()?;
    let entries = obj
        .get("versions")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_object)
                .filter_map(|e| {
                    let id = e.get("id")?.as_str()?.to_string();
                    let num = |k: &str| e.get(k).and_then(Value::as_f64).unwrap_or(0.0);
                    let mut extra = e.clone();
                    for k in ["id", "label", "created_at", "modified_at"] {
                        extra.remove(k);
                    }
                    Some(Entry {
                        label: e
                            .get("label")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        created_at: num("created_at"),
                        modified_at: num("modified_at"),
                        id,
                        extra,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(Manifest {
        active: obj
            .get("active_version_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        entries,
    })
}

/// 写法同上游：`json.dumps(ensure_ascii=False, indent=2)`，键序 active_version_id、versions，结尾不换行
pub(super) fn render_manifest(m: &Manifest) -> String {
    let versions: Vec<Value> = m
        .entries
        .iter()
        .map(|e| {
            let mut obj = Map::new();
            obj.insert("id".into(), e.id.clone().into());
            obj.insert("label".into(), e.label.clone().into());
            obj.insert("created_at".into(), e.created_at.into());
            obj.insert("modified_at".into(), e.modified_at.into());
            obj.extend(e.extra.clone());
            Value::Object(obj)
        })
        .collect();
    let mut root = Map::new();
    root.insert(
        "active_version_id".into(),
        m.active.clone().map_or(Value::Null, Value::from),
    );
    root.insert("versions".into(), Value::Array(versions));
    serde_json::to_string_pretty(&Value::Object(root)).unwrap_or_default()
}

async fn load_manifest(host: &dyn Host, p: &Paths) -> Result<Manifest, AppFrameworkError> {
    match read_text(host, &p.versions.join(MANIFEST)).await? {
        None => Ok(Manifest::default()),
        Some(raw) => parse_manifest(&raw).ok_or_else(|| {
            AppFrameworkError::Integration("提示词的版本清单坏了，打开「原始文件」看看".into())
        }),
    }
}

async fn save_manifest(host: &dyn Host, p: &Paths, m: &Manifest) -> Result<(), AppFrameworkError> {
    write_text(
        host,
        &p.versions,
        &p.versions.join(MANIFEST),
        &render_manifest(m),
    )
    .await
}

fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// 版本号同上游：本地时间 `v%Y%m%d%H%M%S`，同一秒撞了往后加 -2、-3
async fn new_version_id(host: &dyn Host, p: &Paths) -> Result<String, AppFrameworkError> {
    let base = chrono::Local::now().format("v%Y%m%d%H%M%S").to_string();
    let mut id = base.clone();
    let mut n = 2;
    while host
        .exists(&p.versions.join(format!("{id}.prompt")))
        .await
        .map_err(host_err)?
    {
        id = format!("{base}-{n}");
        n += 1;
    }
    Ok(id)
}

async fn default_of(host: &dyn Host, p: &Paths, name: &str) -> Result<String, AppFrameworkError> {
    read_text(host, &p.default)
        .await?
        .ok_or_else(|| AppFrameworkError::Validation(format!("没有这个提示词：{name}")))
}

pub(super) async fn catalog(
    host: &dyn Host,
    instance: &AppInstance,
) -> Result<MaiBotPromptCatalog, AppFrameworkError> {
    let root = HostPath::from_posix(&instance.install_dir);
    let prompts_root = root.join(PROMPTS_DIR);
    let mut languages = Vec::new();
    if host.exists(&prompts_root).await.map_err(host_err)? {
        let mut dirs: Vec<String> = host
            .list_dir(&prompts_root)
            .await
            .map_err(host_err)?
            .into_iter()
            .filter(|e| e.is_dir)
            .map(|e| e.name)
            .collect();
        dirs.sort();
        for language in dirs {
            let prompts = language_prompts(host, &root, &language).await?;
            languages.push(MaiBotPromptLanguage { language, prompts });
        }
    }
    Ok(MaiBotPromptCatalog {
        languages,
        active_language: ACTIVE_LANGUAGE.into(),
        live: false,
    })
}

async fn names_in(host: &dyn Host, dir: &HostPath) -> Result<BTreeSet<String>, AppFrameworkError> {
    if !host.exists(dir).await.map_err(host_err)? {
        return Ok(BTreeSet::new());
    }
    Ok(host
        .list_dir(dir)
        .await
        .map_err(host_err)?
        .into_iter()
        .map(|e| e.name)
        .collect())
}

async fn language_prompts(
    host: &dyn Host,
    root: &HostPath,
    language: &str,
) -> Result<Vec<MaiBotPromptInfo>, AppFrameworkError> {
    let dir = root.join(PROMPTS_DIR).join(language);
    let entries = host.list_dir(&dir).await.map_err(host_err)?;
    let meta = read_text(host, &dir.join(META))
        .await?
        .and_then(|t| t.parse::<toml::Table>().ok())
        .unwrap_or_default();
    let custom_dir = root.join(CUSTOM_DIR).join(language);
    let customized = names_in(host, &custom_dir).await?;
    let with_versions = names_in(host, &custom_dir.join(VERSIONS_DIR)).await?;

    let mut names: Vec<String> = entries
        .iter()
        .filter(|e| !e.is_dir && e.name.ends_with(".prompt"))
        .map(|e| e.name.clone())
        .collect();
    names.sort();
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        let stem = name.trim_end_matches(".prompt").to_string();
        // 同目录 `<名>.meta.toml` 比 `.meta.toml` 优先，逐键合并
        let own_meta = if entries
            .iter()
            .any(|e| e.name == format!("{stem}.meta.toml"))
        {
            read_text(host, &dir.join(format!("{stem}.meta.toml")))
                .await?
                .and_then(|t| t.parse::<toml::Table>().ok())
        } else {
            None
        };
        let pick = |key: &str| {
            own_meta
                .as_ref()
                .and_then(|m| meta_value(m, &stem, key))
                .or_else(|| meta_value(&meta, &stem, key))
        };
        let is_custom = customized.contains(&name);
        let version_count = if with_versions.contains(&stem) || is_custom {
            let p = paths(root, language, &name);
            versions_of(host, &p, is_custom).await?.len() as u32
        } else {
            0
        };
        out.push(MaiBotPromptInfo {
            display_name: pick("display_name")
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default(),
            description: pick("description")
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default(),
            advanced: pick("advanced").and_then(|v| v.as_bool()).unwrap_or(false),
            customized: is_custom,
            version_count,
            name,
        });
    }
    Ok(out)
}

/// 上游 `_extract_template_metadata`：先找 `templates.<名>`，再找 `[<名>]`，再看整张表有没有这几个键
fn meta_value(table: &toml::Table, stem: &str, key: &str) -> Option<toml::Value> {
    let scoped = table
        .get("templates")
        .and_then(|t| t.get(stem))
        .or_else(|| table.get(stem))
        .and_then(toml::Value::as_table);
    match scoped {
        Some(t) => t.get(key).cloned(),
        None if ["display_name", "advanced", "description"]
            .iter()
            .any(|k| table.contains_key(*k)) =>
        {
            table.get(key).cloned()
        }
        None => None,
    }
}

/// 清单里文件还在的条目；有覆盖、清单没记在用、一条也没有时补一条旧格式的
async fn versions_of(
    host: &dyn Host,
    p: &Paths,
    customized: bool,
) -> Result<Vec<MaiBotPromptVersion>, AppFrameworkError> {
    let m = load_manifest(host, p).await?;
    let mut out = Vec::new();
    for e in &m.entries {
        if host
            .exists(&p.versions.join(format!("{}.prompt", e.id)))
            .await
            .map_err(host_err)?
        {
            out.push(MaiBotPromptVersion {
                label: if e.label.is_empty() {
                    e.id.clone()
                } else {
                    e.label.clone()
                },
                id: e.id.clone(),
                created_at: e.created_at,
                modified_at: e.modified_at,
                active: m.active.as_deref() == Some(e.id.as_str()),
            });
        }
    }
    if customized && m.active.is_none() && out.is_empty() {
        out.push(MaiBotPromptVersion {
            id: LEGACY_ID.into(),
            label: LEGACY_LABEL.into(),
            created_at: 0.0,
            modified_at: 0.0,
            active: true,
        });
    }
    out.sort_by(|a, b| b.modified_at.total_cmp(&a.modified_at));
    Ok(out)
}

pub(super) async fn file(
    host: &dyn Host,
    instance: &AppInstance,
    language: &str,
    name: &str,
) -> Result<MaiBotPromptFile, AppFrameworkError> {
    let p = paths(&root_of(instance), language, name);
    let default_content = default_of(host, &p, name).await?;
    let custom = read_text(host, &p.custom).await?;
    let m = load_manifest(host, &p).await?;
    let versions = versions_of(host, &p, custom.is_some()).await?;
    let active_version_id = m
        .active
        .clone()
        .or_else(|| custom.as_ref().map(|_| LEGACY_ID.to_string()));
    Ok(MaiBotPromptFile {
        language: language.into(),
        name: name.into(),
        customized: custom.is_some(),
        content: custom.unwrap_or_else(|| default_content.clone()),
        default_content,
        active_version_id,
        versions,
    })
}

pub(super) async fn version(
    host: &dyn Host,
    instance: &AppInstance,
    language: &str,
    name: &str,
    version_id: &str,
) -> Result<String, AppFrameworkError> {
    let p = paths(&root_of(instance), language, name);
    version_content(host, &p, version_id).await
}

async fn version_content(
    host: &dyn Host,
    p: &Paths,
    id: &str,
) -> Result<String, AppFrameworkError> {
    let path = if id == LEGACY_ID {
        p.custom.clone()
    } else {
        p.versions.join(format!("{id}.prompt"))
    };
    read_text(host, &path)
        .await?
        .ok_or_else(|| AppFrameworkError::Validation("这个版本已经不在了".into()))
}

pub(super) async fn act(
    host: &dyn Host,
    instance: &AppInstance,
    action: &MaiBotPromptAction,
) -> Result<MaiBotPromptFile, AppFrameworkError> {
    let (language, name) = action.target();
    let p = paths(&root_of(instance), language, name);
    let default = default_of(host, &p, name).await?;
    let mut m = load_manifest(host, &p).await?;
    match action {
        MaiBotPromptAction::Save {
            content,
            label,
            version_id,
            ..
        } => {
            check_prompt(content, &default).map_err(AppFrameworkError::Validation)?;
            let now = now_secs();
            let label = label.trim();
            let id = match version_id.as_deref().filter(|id| *id != LEGACY_ID) {
                Some(id) => {
                    let entry = m.entries.iter_mut().find(|e| e.id == id).ok_or_else(|| {
                        AppFrameworkError::Validation("这个版本已经不在了".into())
                    })?;
                    if !label.is_empty() {
                        entry.label = label.to_string();
                    }
                    entry.modified_at = now;
                    id.to_string()
                }
                None => {
                    let id = new_version_id(host, &p).await?;
                    let stem = name.trim_end_matches(".prompt");
                    let label = if label.is_empty() {
                        format!(
                            "{stem} 自定义版本 {}",
                            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
                        )
                    } else {
                        label.to_string()
                    };
                    m.entries.push(Entry {
                        id: id.clone(),
                        label,
                        created_at: now,
                        modified_at: now,
                        extra: Map::new(),
                    });
                    id
                }
            };
            write_text(
                host,
                &p.versions,
                &p.versions.join(format!("{id}.prompt")),
                content,
            )
            .await?;
            m.active = Some(id);
            save_manifest(host, &p, &m).await?;
            write_text(host, &p.custom_dir, &p.custom, content).await?;
        }
        MaiBotPromptAction::Activate { version_id, .. } => {
            let content = version_content(host, &p, version_id).await?;
            check_prompt(&content, &default).map_err(AppFrameworkError::Validation)?;
            write_text(host, &p.custom_dir, &p.custom, &content).await?;
            if version_id != LEGACY_ID {
                m.active = Some(version_id.clone());
                save_manifest(host, &p, &m).await?;
            }
        }
        MaiBotPromptAction::DeleteVersion { version_id, .. } if version_id == LEGACY_ID => {
            if !host.exists(&p.custom).await.map_err(host_err)? {
                return Err(AppFrameworkError::Validation("这个版本已经不在了".into()));
            }
            host.remove_file(&p.custom).await.map_err(host_err)?;
            if m.active.is_none() {
                save_manifest(host, &p, &m).await?;
            }
        }
        MaiBotPromptAction::DeleteVersion { version_id, .. } => {
            let file = p.versions.join(format!("{version_id}.prompt"));
            let Some(pos) = m.entries.iter().position(|e| &e.id == version_id) else {
                return Err(AppFrameworkError::Validation("这个版本已经不在了".into()));
            };
            if !host.exists(&file).await.map_err(host_err)? {
                return Err(AppFrameworkError::Validation("这个版本已经不在了".into()));
            }
            host.remove_file(&file).await.map_err(host_err)?;
            m.entries.remove(pos);
            if m.active.as_deref() == Some(version_id.as_str()) {
                m.active = None;
                if host.exists(&p.custom).await.map_err(host_err)? {
                    host.remove_file(&p.custom).await.map_err(host_err)?;
                }
            }
            save_manifest(host, &p, &m).await?;
        }
        MaiBotPromptAction::Restore { .. } => {
            if host.exists(&p.custom).await.map_err(host_err)? {
                host.remove_file(&p.custom).await.map_err(host_err)?;
                m.active = None;
                save_manifest(host, &p, &m).await?;
            }
        }
    }
    file(host, instance, language, name).await
}
