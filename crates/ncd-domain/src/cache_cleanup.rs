//! Desktop 自有下载缓存；IPC 只接受类别，不接受删除路径。
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum CacheCategory {
    Installers,
    DesktopUpdates,
    DownloadResidues,
}

impl CacheCategory {
    pub const ALL: [Self; 3] = [
        Self::Installers,
        Self::DesktopUpdates,
        Self::DownloadResidues,
    ];
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct CacheCategoryUsage {
    pub category: CacheCategory,
    pub files: u32,
    pub bytes: u64,
    pub skipped: u32,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct CacheScan {
    pub categories: Vec<CacheCategoryUsage>,
    // 仅供界面提示；清理时仍会原子取得独占锁。
    pub busy: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct CacheCleanupResult {
    pub deleted_files: u32,
    // 成功删除文件的逻辑大小，不等同于压缩、稀疏文件实际释放的磁盘块。
    pub deleted_bytes: u64,
    pub skipped_files: u32,
    pub failed_files: u32,
    pub warnings: Vec<String>,
}
