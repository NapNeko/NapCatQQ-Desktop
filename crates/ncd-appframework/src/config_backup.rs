//! 原文配置备份。配置路径白名单与发现规则共用，绝不遍历程序或数据库目录。

use std::collections::{BTreeMap, HashSet};
use std::future::Future;
use std::sync::Arc;

use ncd_domain::{AppConfigFormat, AppInstance};
use ncd_host::{DirEntry, Host, HostPath, Os};
use serde::{Deserialize, Serialize};

use crate::AppFrameworkAdapter;
use crate::config_doc::validate_text;

pub const MAX_CONFIG_FILE_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_CONFIG_BACKUP_BYTES: usize = 8 * 1024 * 1024;
const MAX_CONFIG_FILES: usize = 1024;
const MAX_DISCOVERED_ENTRIES: usize = 8192;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameworkConfigFile {
    pub relative_path: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameworkConfigBackup {
    pub version: u32,
    pub instance_id: String,
    pub framework_id: String,
    pub files: Vec<FrameworkConfigFile>,
}

/// Desktop 上的恢复检查点。导出只取 backup，导入永远重新判断 pending。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameworkConfigRecovery {
    pub backup: FrameworkConfigBackup,
    pub pending: bool,
}

pub fn safe_instance_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 160
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

fn safe_part(part: &str) -> bool {
    if part.is_empty()
        || matches!(part, "." | "..")
        || part.ends_with(['.', ' '])
        || part
            .chars()
            .any(|c| c.is_control() || "\\:<>\"|?*".contains(c))
    {
        return false;
    }
    let stem = part
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    !matches!(stem.as_str(), "con" | "prn" | "aux" | "nul")
        && !["com", "lpt"].iter().any(|prefix| {
            stem.strip_prefix(prefix).is_some_and(|suffix| {
                suffix.len() == 1 && matches!(suffix.as_bytes()[0], b'1'..=b'9')
            })
        })
}

fn safe_relative_path(path: &str) -> bool {
    path.len() <= 512 && path.split('/').all(safe_part)
}

fn config_format(path: &str) -> Option<AppConfigFormat> {
    match path.rsplit('.').next()? {
        "json" => Some(AppConfigFormat::Json),
        "toml" => Some(AppConfigFormat::Toml),
        "yaml" | "yml" => Some(AppConfigFormat::Yaml),
        _ => None,
    }
}

fn excluded_part(part: &str) -> bool {
    matches!(
        part.to_ascii_lowercase().as_str(),
        "node_modules"
            | ".git"
            | "__pycache__"
            | "cache"
            | "caches"
            | "logs"
            | "temp"
            | "tmp"
            | "template"
            | "templates"
            | "default_config"
    )
}

fn dotenv(path: &str) -> bool {
    !path.contains('/')
        && (path == ".env" || path.starts_with(".env."))
        && !path.ends_with(".bak")
        && !path.ends_with(".tmp")
}

/// 仅由路径决定格式，包内不能把程序伪装成另一种配置格式。
pub fn backup_file_format(framework: &str, path: &str) -> Result<Option<AppConfigFormat>, String> {
    if !safe_relative_path(path) || path.split('/').any(excluded_part) {
        return Err(format!("非法框架配置路径: {path}"));
    }
    if !matches!(
        framework,
        "karin" | "nonebot2" | "astrbot" | "maibot" | "koishi" | "yunzai"
    ) {
        return Err(format!("不支持的框架配置备份: {framework}"));
    }
    if dotenv(path) && matches!(framework, "karin" | "nonebot2" | "koishi" | "yunzai") {
        return Ok(Some(AppConfigFormat::DotEnv));
    }
    let parts: Vec<_> = path.split('/').collect();
    let allowed = match framework {
        "karin" => {
            path == "package.json"
                || (parts.first() == Some(&"@karinjs")
                    && parts.len() >= 3
                    && (parts[1] == "config" || parts.len() == 3 || parts[2] == "config"))
        }
        "nonebot2" => path == "pyproject.toml",
        // 多配置 abconf_*.json 和插件 *_config.json 都在 data/config，映射保存在 shared_preferences。
        "astrbot" => {
            matches!(
                path,
                "data/cmd_config.json" | "data/shared_preferences.json"
            ) || (path.starts_with("data/config/") && parts.len() == 3 && path.ends_with(".json"))
        }
        "maibot" => {
            matches!(
                path,
                "config/bot_config.toml" | "config/model_config.toml" | "data/webui.json"
            ) || (parts.len() == 3 && parts[0] == "plugins" && parts[2] == "config.toml")
                || path.starts_with("data/custom_prompts/")
        }
        "koishi" => matches!(path, "koishi.yml" | "package.json"),
        "yunzai" => {
            path == "package.json"
                || path.starts_with("config/config/")
                || (parts.len() >= 4 && parts[0] == "plugins" && parts[2] == "config")
        }
        _ => false,
    };
    if allowed && framework == "maibot" && path.starts_with("data/custom_prompts/") {
        if path.ends_with(".prompt") {
            return Ok(None);
        }
        let leaf = parts.last().copied().unwrap_or_default();
        if leaf != "manifest.json" && leaf != ".meta.toml" && !leaf.ends_with(".meta.toml") {
            return Err(format!("不是自定义提示词或其元数据: {path}"));
        }
    }
    if allowed && let Some(format) = config_format(path) {
        return Ok(Some(format));
    }
    Err(format!("不在 {framework} 配置备份范围内: {path}"))
}

impl FrameworkConfigBackup {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || !safe_instance_id(&self.instance_id)
            || self.files.len() > MAX_CONFIG_FILES
        {
            return Err("框架配置备份版本、实例 ID 或文件数量无效".into());
        }
        // 即使是空快照也验证框架 ID。
        if !matches!(
            self.framework_id.as_str(),
            "karin" | "nonebot2" | "astrbot" | "maibot" | "koishi" | "yunzai"
        ) {
            return Err(format!("不支持的框架配置备份: {}", self.framework_id));
        }
        let mut paths = HashSet::new();
        let mut total = 0usize;
        for file in &self.files {
            let key = file.relative_path.to_ascii_lowercase();
            if !paths.insert(key) {
                return Err(format!("框架配置备份含重复路径: {}", file.relative_path));
            }
            let format = backup_file_format(&self.framework_id, &file.relative_path)?;
            total = total
                .checked_add(file.text.len())
                .ok_or("框架配置大小超限")?;
            if file.text.len() > MAX_CONFIG_FILE_BYTES || total > MAX_CONFIG_BACKUP_BYTES {
                return Err("框架配置超出大小限制（单文件 2 MiB、单实例 8 MiB）".into());
            }
            if let Some(format) = format {
                // AstrBot 也会写 UTF-8 BOM，校验时忽略，恢复时仍原样保留。
                validate_text(format, file.text.trim_start_matches('\u{feff}'))
                    .map_err(|e| format!("{}: {e}", file.relative_path))?;
            }
        }
        Ok(())
    }
}

fn descend(framework: &str, rel: &str) -> bool {
    if !safe_relative_path(rel) || rel.split('/').any(excluded_part) {
        return false;
    }
    let parts: Vec<_> = rel.split('/').collect();
    match framework {
        "karin" => {
            rel == "@karinjs"
                || (parts.first() == Some(&"@karinjs")
                    && (parts.len() == 2
                        || parts[1] == "config"
                        || parts.get(2) == Some(&"config")))
        }
        "astrbot" => matches!(rel, "data" | "data/config"),
        "maibot" => {
            matches!(rel, "config" | "data" | "plugins")
                || (parts[0] == "plugins" && parts.len() == 2)
                || rel == "data/custom_prompts"
                || rel.starts_with("data/custom_prompts/")
        }
        "yunzai" => {
            matches!(rel, "config" | "config/config" | "plugins")
                || rel.starts_with("config/config/")
                || (parts[0] == "plugins" && (parts.len() == 2 || parts.get(2) == Some(&"config")))
        }
        _ => false,
    }
}

fn matching_entry<'a>(entries: &'a [DirEntry], name: &str, os: Os) -> Option<&'a DirEntry> {
    entries.iter().find(|entry| {
        if os == Os::Windows {
            entry.name.eq_ignore_ascii_case(name)
        } else {
            entry.name == name
        }
    })
}

/// 包中的安装路径只有已存在且没有符号链接的目录才能作为目标。
pub async fn checked_install_root(host: &dyn Host, root: &HostPath) -> Result<bool, String> {
    let raw = root.as_posix().trim_end_matches('/');
    if !raw.starts_with('/') || raw.len() > 4096 {
        return Err("框架安装路径必须是绝对目录".into());
    }
    let parts: Vec<_> = raw[1..].split('/').collect();
    if !parts.iter().all(|part| safe_part(part)) {
        return Err("框架安装路径含非法路径段".into());
    }
    let (mut cursor, start) = if host.os() == Os::Windows {
        if parts.len() < 2 || parts[0].len() != 1 || !parts[0].as_bytes()[0].is_ascii_alphabetic() {
            return Err("框架安装路径缺少有效盘符或项目目录".into());
        }
        (HostPath::from_posix(format!("/{}/", parts[0])), 1)
    } else {
        (HostPath::from_posix("/"), 0)
    };
    for part in &parts[start..] {
        let entries = host
            .list_dir(&cursor)
            .await
            .map_err(|e| format!("检查框架安装目录失败: {e}"))?;
        let Some(entry) = matching_entry(&entries, part, host.os()) else {
            return Ok(false);
        };
        if entry.is_symlink || !entry.is_dir {
            return Err(format!("框架安装路径不能包含链接或文件: {cursor}/{part}"));
        }
        cursor = cursor.join(part);
    }
    Ok(true)
}

async fn checked_file(
    host: &dyn Host,
    root: &HostPath,
    rel: &str,
) -> Result<Option<DirEntry>, String> {
    let parts: Vec<_> = rel.split('/').collect();
    let mut cursor = root.clone();
    for (i, part) in parts.iter().enumerate() {
        let entries = host
            .list_dir(&cursor)
            .await
            .map_err(|e| format!("检查 {rel} 失败: {e}"))?;
        let Some(entry) = matching_entry(&entries, part, host.os()) else {
            return Ok(None);
        };
        if entry.is_symlink
            || (i + 1 < parts.len() && !entry.is_dir)
            || (i + 1 == parts.len() && entry.is_dir)
        {
            return Err(format!("框架配置路径不能包含链接或错误文件类型: {rel}"));
        }
        if i + 1 == parts.len() {
            return Ok(Some(entry.clone()));
        }
        cursor = cursor.join(part);
    }
    Ok(None)
}

async fn read_bounded(
    host: &dyn Host,
    path: &HostPath,
    entry: &DirEntry,
) -> Result<Vec<u8>, String> {
    if entry.size > MAX_CONFIG_FILE_BYTES as u64 {
        return Err(format!("框架配置文件超过 2 MiB: {path}"));
    }
    let bytes = host
        .read_file(path)
        .await
        .map_err(|e| format!("读取框架配置 {path} 失败: {e}"))?;
    if bytes.len() > MAX_CONFIG_FILE_BYTES {
        return Err(format!("框架配置文件超过 2 MiB: {path}"));
    }
    Ok(bytes.to_vec())
}

pub async fn capture_config_backup(
    host: &dyn Host,
    adapter: &dyn AppFrameworkAdapter,
    instance: &AppInstance,
) -> Result<FrameworkConfigBackup, String> {
    let root = HostPath::from_posix(&instance.install_dir);
    if !checked_install_root(host, &root).await? {
        return Err("框架安装目录不存在".into());
    }
    let framework = instance.framework_id.as_str();
    let mut rels = BTreeMap::<String, DirEntry>::new();
    let mut directories = vec![String::new()];
    let mut visited = 0usize;
    while let Some(rel) = directories.pop() {
        let dir = if rel.is_empty() {
            root.clone()
        } else {
            root.join(&rel)
        };
        for entry in host
            .list_dir(&dir)
            .await
            .map_err(|e| format!("读取框架配置目录失败 {dir}: {e}"))?
        {
            visited += 1;
            if visited > MAX_DISCOVERED_ENTRIES {
                return Err("框架配置目录条目过多".into());
            }
            let path = if rel.is_empty() {
                entry.name.clone()
            } else {
                format!("{rel}/{}", entry.name)
            };
            let config = backup_file_format(framework, &path).is_ok();
            let directory = descend(framework, &path);
            if entry.is_symlink && (config || directory) {
                return Err(format!("框架配置不能备份符号链接: {path}"));
            }
            if entry.is_dir && directory {
                if path.split('/').count() > 10 {
                    return Err(format!("框架配置目录层级过深: {path}"));
                }
                directories.push(path);
            } else if !entry.is_dir && config {
                rels.insert(path, entry);
            }
        }
    }
    // 新框架的主文档必须显式加入上面的可恢复白名单，不能只导出却无法安全导入。
    for doc in adapter.config_documents(instance) {
        backup_file_format(framework, &doc.rel_path)?;
        if !rels.contains_key(&doc.rel_path)
            && let Some(entry) = checked_file(host, &root, &doc.rel_path).await?
        {
            rels.insert(doc.rel_path, entry);
        }
    }
    let mut backup = FrameworkConfigBackup {
        version: 1,
        instance_id: instance.id.as_str().into(),
        framework_id: framework.into(),
        files: Vec::new(),
    };
    let mut total = 0usize;
    for (relative_path, entry) in rels {
        if backup.files.len() >= MAX_CONFIG_FILES {
            return Err("框架配置文件数量超过 1024".into());
        }
        let bytes = read_bounded(host, &root.join(&relative_path), &entry).await?;
        total += bytes.len();
        if total > MAX_CONFIG_BACKUP_BYTES {
            return Err("单实例框架配置超过 8 MiB".into());
        }
        let text = String::from_utf8(bytes)
            .map_err(|_| format!("框架配置不是 UTF-8 文本: {relative_path}"))?;
        backup.files.push(FrameworkConfigFile {
            relative_path,
            text,
        });
    }
    backup.validate()?;
    for file in &backup.files {
        let entry = checked_file(host, &root, &file.relative_path)
            .await?
            .ok_or_else(|| format!("备份期间框架配置已删除: {}", file.relative_path))?;
        if read_bounded(host, &root.join(&file.relative_path), &entry).await?
            != file.text.as_bytes()
        {
            return Err(format!(
                "备份期间框架配置已变更，请重新导出: {}",
                file.relative_path
            ));
        }
    }
    Ok(backup)
}

struct ConfigWrite {
    host: Arc<dyn Host>,
    root: HostPath,
    relative_path: String,
    original: Option<Vec<u8>>,
    text: String,
}

/// 在任何写入前捕获所有主机的原文件。提交失败时连新建文件一起回滚。
#[derive(Default)]
pub struct FrameworkConfigRestorePlan {
    writes: Vec<ConfigWrite>,
}

impl FrameworkConfigRestorePlan {
    pub async fn add(
        &mut self,
        host: Arc<dyn Host>,
        instance: &AppInstance,
        backup: &FrameworkConfigBackup,
    ) -> Result<(), String> {
        backup.validate()?;
        if backup.instance_id != instance.id.as_str()
            || backup.framework_id != instance.framework_id.as_str()
        {
            return Err("框架配置备份与目标实例不匹配".into());
        }
        let root = HostPath::from_posix(&instance.install_dir);
        if !checked_install_root(host.as_ref(), &root).await? {
            return Err("框架安装目录不存在".into());
        }
        let mut writes = Vec::new();
        for file in &backup.files {
            let original = match checked_file(host.as_ref(), &root, &file.relative_path).await? {
                Some(entry) => Some(
                    read_bounded(host.as_ref(), &root.join(&file.relative_path), &entry).await?,
                ),
                None => None,
            };
            if self.writes.iter().any(|write| {
                write.host.id() == host.id()
                    && if host.os() == Os::Windows {
                        write
                            .root
                            .join(&write.relative_path)
                            .as_posix()
                            .eq_ignore_ascii_case(root.join(&file.relative_path).as_posix())
                    } else {
                        write.root.join(&write.relative_path) == root.join(&file.relative_path)
                    }
            }) {
                return Err(format!(
                    "多个实例指向同一框架配置文件: {}",
                    file.relative_path
                ));
            }
            writes.push(ConfigWrite {
                host: Arc::clone(&host),
                root: root.clone(),
                relative_path: file.relative_path.clone(),
                original,
                text: file.text.clone(),
            });
        }
        let original_bytes: usize = self
            .writes
            .iter()
            .chain(&writes)
            .filter_map(|w| w.original.as_ref())
            .map(Vec::len)
            .sum();
        if original_bytes > 64 * 1024 * 1024 {
            return Err("框架配置原文件超过 64 MiB 回滚预算".into());
        }
        self.writes.extend(writes);
        Ok(())
    }

    pub async fn commit_with<T>(
        self,
        commit: impl Future<Output = Result<T, String>>,
    ) -> Result<T, String> {
        // 准备到提交之间若有人直接改了文件，拒绝覆盖；也重新检查目录链接。
        for write in &self.writes {
            if !checked_install_root(write.host.as_ref(), &write.root).await? {
                return Err("框架安装目录在提交前消失".into());
            }
            let current =
                match checked_file(write.host.as_ref(), &write.root, &write.relative_path).await? {
                    Some(entry) => Some(
                        read_bounded(
                            write.host.as_ref(),
                            &write.root.join(&write.relative_path),
                            &entry,
                        )
                        .await?,
                    ),
                    None => None,
                };
            if current != write.original {
                return Err(format!(
                    "框架配置已变更，请重新导入: {}",
                    write.relative_path
                ));
            }
        }
        for (index, write) in self.writes.iter().enumerate() {
            let path = write.root.join(&write.relative_path);
            let result = async {
                if let Some(parent) = path.parent() {
                    write
                        .host
                        .create_dir_all(&parent)
                        .await
                        .map_err(|e| e.to_string())?;
                }
                write
                    .host
                    .write_file(&path, write.text.as_bytes())
                    .await
                    .map_err(|e| e.to_string())
            }
            .await;
            if let Err(error) = result {
                return Err(self
                    .rollback(index + 1, format!("恢复框架配置 {path} 失败: {error}"))
                    .await);
            }
        }
        match commit.await {
            Ok(result) => Ok(result),
            Err(error) => Err(self.rollback(self.writes.len(), error).await),
        }
    }

    async fn rollback(&self, count: usize, error: String) -> String {
        let mut failures = Vec::new();
        for write in self.writes[..count].iter().rev() {
            let path = write.root.join(&write.relative_path);
            let result = match &write.original {
                Some(bytes) => write.host.write_file(&path, bytes).await,
                None => match write.host.exists(&path).await {
                    Ok(true) => write.host.remove_file(&path).await,
                    Ok(false) => Ok(()),
                    Err(error) => Err(error),
                },
            };
            if let Err(e) = result {
                failures.push(format!("{path}: {e}"));
            }
        }
        if failures.is_empty() {
            format!("{error}（框架配置已回滚）")
        } else {
            format!(
                "{error}；部分框架配置回滚失败，请检查连接和文件权限: {}",
                failures.join("；")
            )
        }
    }
}

#[cfg(test)]
#[path = "config_backup_tests.rs"]
mod tests;
