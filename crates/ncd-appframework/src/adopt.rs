//! 导入项目的文件快照：存在 Desktop 数据根，不写进用户仓库。
//! 释放接管时按快照还原；写入失败仍靠内存回滚。

use ncd_domain::AppInstance;
use ncd_host::{Host, HostPath};
use ncd_traits::AppFrameworkError;
use serde::{Deserialize, Serialize};

use crate::config_doc::ensure_parent_dir;
use crate::node_tooling::TOOLS_DIR;
use crate::uv_tooling::UV_MARKER_FILE;

pub const MAX_ADOPT_FILE_BYTES: usize = 512 * 1024;

const NODE_MARKER: &str = ".ncd-node";
const APP_PID: &str = ".ncd-app.pid";
const NB_LOG: &str = ".ncd-nonebot2.log";
const KARIN_LOG: &str = ".ncd-karin.log";

const NCD_FILES: &[&str] = &[
    UV_MARKER_FILE,
    NODE_MARKER,
    APP_PID,
    NB_LOG,
    KARIN_LOG,
];
const NCD_DIRS: &[&str] = &[TOOLS_DIR];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdoptedFile {
    pub rel_path: String,
    pub existed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdoptRestoreScope {
    All,
    /// 对接会改的 dotenv / adapter.json，不动 bot.py / pyproject
    Link,
}

impl AdoptRestoreScope {
    pub fn includes(self, rel: &str) -> bool {
        match self {
            Self::All => true,
            Self::Link => {
                rel == ".env"
                    || rel.starts_with(".env.")
                    || rel.ends_with("adapter.json")
            }
        }
    }
}

/// 导入实例不在用户目录落 `<file>.ncd.bak`，避免弄脏 git。
pub fn write_project_sidecar(instance: &AppInstance) -> bool {
    !instance.origin.is_imported()
}

pub fn is_dotenv_name(name: &str) -> bool {
    (name == ".env" || name.starts_with(".env.")) && !name.ends_with(".ncd.bak")
}

pub async fn list_dotenv_rels(
    host: &dyn Host,
    root: &HostPath,
) -> Result<Vec<String>, AppFrameworkError> {
    let entries = host
        .list_dir(root)
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    let mut rels: Vec<String> = entries
        .into_iter()
        .filter(|e| !e.is_dir && is_dotenv_name(&e.name))
        .map(|e| e.name)
        .collect();
    if !rels.iter().any(|r| r == ".env") {
        rels.push(".env".into());
    }
    rels.sort();
    rels.dedup();
    Ok(rels)
}

pub fn merge_rels(base: Vec<String>, extra: &[&str]) -> Vec<String> {
    let mut rels = base;
    for item in extra {
        if !rels.iter().any(|r| r == item) {
            rels.push((*item).to_string());
        }
    }
    rels.sort();
    rels.dedup();
    rels
}

pub async fn capture_adopted_files(
    host: &dyn Host,
    root: &HostPath,
    rels: &[String],
) -> Result<Vec<AdoptedFile>, AppFrameworkError> {
    let mut files = Vec::with_capacity(rels.len());
    for rel in rels {
        if rel.is_empty() || rel.contains("..") {
            continue;
        }
        let path = root.join(rel);
        let existed = host
            .exists(&path)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
        if !existed {
            files.push(AdoptedFile {
                rel_path: rel.clone(),
                existed: false,
                text: None,
                skip_reason: None,
            });
            continue;
        }
        let bytes = host
            .read_file(&path)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
        if bytes.len() > MAX_ADOPT_FILE_BYTES {
            files.push(AdoptedFile {
                rel_path: rel.clone(),
                existed: true,
                text: None,
                skip_reason: Some(format!("超过 {} 字节，跳过", MAX_ADOPT_FILE_BYTES)),
            });
            continue;
        }
        match String::from_utf8(bytes.to_vec()) {
            Ok(text) => files.push(AdoptedFile {
                rel_path: rel.clone(),
                existed: true,
                text: Some(text),
                skip_reason: None,
            }),
            Err(_) => files.push(AdoptedFile {
                rel_path: rel.clone(),
                existed: true,
                text: None,
                skip_reason: Some("不是 UTF-8，跳过".into()),
            }),
        }
    }
    Ok(files)
}

pub async fn restore_adopted_files(
    host: &dyn Host,
    root: &HostPath,
    files: &[AdoptedFile],
    scope: AdoptRestoreScope,
) -> Result<(), AppFrameworkError> {
    for file in files {
        if !scope.includes(&file.rel_path) {
            continue;
        }
        if file.skip_reason.is_some() {
            continue;
        }
        let path = root.join(&file.rel_path);
        if !file.existed {
            if host
                .exists(&path)
                .await
                .map_err(|e| AppFrameworkError::Host(e.to_string()))?
            {
                host.remove_file(&path)
                    .await
                    .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
            }
            continue;
        }
        let Some(text) = file.text.as_ref() else {
            continue;
        };
        ensure_parent_dir(host, &path).await?;
        host.write_file(&path, text.as_bytes())
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    }
    Ok(())
}

/// 清掉桌面端落在项目里的标记 / 日志 / sidecar，不碰快照里原本就有的路径。
pub async fn remove_ncd_debris(
    host: &dyn Host,
    root: &HostPath,
    keep: &[AdoptedFile],
) -> Result<(), AppFrameworkError> {
    let keep_existed: Vec<&str> = keep
        .iter()
        .filter(|f| f.existed)
        .map(|f| f.rel_path.as_str())
        .collect();
    let skip = |rel: &str| keep_existed.iter().any(|k| *k == rel);

    for rel in NCD_FILES {
        if skip(rel) {
            continue;
        }
        remove_if_exists(host, &root.join(rel)).await?;
    }
    for rel in NCD_DIRS {
        if skip(rel) {
            continue;
        }
        let path = root.join(rel);
        if host
            .exists(&path)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?
        {
            host.remove_dir_all(&path)
                .await
                .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
        }
    }
    remove_bak_sidecars(host, root, &keep_existed).await?;
    let karin_cfg = root.join("@karinjs/config");
    if host
        .exists(&karin_cfg)
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?
    {
        remove_bak_sidecars(host, &karin_cfg, &keep_existed).await?;
    }
    Ok(())
}

async fn remove_bak_sidecars(
    host: &dyn Host,
    dir: &HostPath,
    keep: &[&str],
) -> Result<(), AppFrameworkError> {
    let entries = match host.list_dir(dir).await {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };
    for entry in entries {
        if entry.is_dir || !entry.name.ends_with(".ncd.bak") {
            continue;
        }
        let rel = entry.name;
        if keep.iter().any(|k| *k == rel || k.ends_with(&format!("/{rel}"))) {
            continue;
        }
        remove_if_exists(host, &dir.join(&rel)).await?;
    }
    Ok(())
}

async fn remove_if_exists(host: &dyn Host, path: &HostPath) -> Result<(), AppFrameworkError> {
    if host
        .exists(path)
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?
    {
        host.remove_file(path)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_host::local::LocalWindowsHost;

    fn host() -> LocalWindowsHost {
        LocalWindowsHost::new()
    }

    fn root_of(dir: &std::path::Path) -> HostPath {
        HostPath::from_windows(dir.to_string_lossy().as_ref())
    }

    #[tokio::test]
    async fn capture_restore_round_trip_and_deletes_created() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        std::fs::write(dir.join(".env"), "PORT=13120\n").unwrap();
        std::fs::write(dir.join("bot.py"), "import nonebot\n").unwrap();
        let host = host();
        let root = root_of(dir);
        let rels = vec![".env".into(), "bot.py".into(), ".env.prod".into()];
        let snap = capture_adopted_files(&host, &root, &rels).await.unwrap();
        assert_eq!(snap.len(), 3);
        assert_eq!(snap[0].rel_path, ".env");
        assert_eq!(snap[0].text.as_deref(), Some("PORT=13120\n"));
        assert!(!snap.iter().find(|f| f.rel_path == ".env.prod").unwrap().existed);

        std::fs::write(dir.join(".env"), "PORT=1\nONEBOT_ACCESS_TOKEN=x\n").unwrap();
        std::fs::write(dir.join(".env.prod"), "DRIVER=~fastapi\n").unwrap();
        std::fs::write(dir.join(".env.ncd.bak"), "stale\n").unwrap();
        std::fs::write(dir.join(".ncd-uv"), "uv\n").unwrap();

        restore_adopted_files(&host, &root, &snap, AdoptRestoreScope::All)
            .await
            .unwrap();
        remove_ncd_debris(&host, &root, &snap).await.unwrap();

        assert_eq!(std::fs::read_to_string(dir.join(".env")).unwrap(), "PORT=13120\n");
        assert_eq!(std::fs::read_to_string(dir.join("bot.py")).unwrap(), "import nonebot\n");
        assert!(!dir.join(".env.prod").exists());
        assert!(!dir.join(".ncd-uv").exists());
        assert!(!dir.join(".env.ncd.bak").exists());
    }

    #[tokio::test]
    async fn link_scope_skips_bot_py() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(".env"), "A=1\n").unwrap();
        std::fs::write(tmp.path().join("bot.py"), "old\n").unwrap();
        let host = host();
        let root = root_of(tmp.path());
        let snap = capture_adopted_files(
            &host,
            &root,
            &[".env".into(), "bot.py".into()],
        )
        .await
        .unwrap();
        std::fs::write(tmp.path().join(".env"), "A=2\n").unwrap();
        std::fs::write(tmp.path().join("bot.py"), "new\n").unwrap();
        restore_adopted_files(&host, &root, &snap, AdoptRestoreScope::Link)
            .await
            .unwrap();
        assert_eq!(std::fs::read_to_string(tmp.path().join(".env")).unwrap(), "A=1\n");
        assert_eq!(std::fs::read_to_string(tmp.path().join("bot.py")).unwrap(), "new\n");
    }

    #[test]
    fn dotenv_name_skips_bak() {
        assert!(is_dotenv_name(".env"));
        assert!(is_dotenv_name(".env.prod"));
        assert!(!is_dotenv_name(".env.ncd.bak"));
        assert!(!is_dotenv_name("bot.py"));
    }
}
