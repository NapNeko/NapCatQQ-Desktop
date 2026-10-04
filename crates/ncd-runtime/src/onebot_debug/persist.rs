//! 调试台落盘数据（工作区、收藏夹、调用历史）的对外入口，都转给 [`DebugStore`]。
//!
//! 存储第一次用到时才读盘：不开调试台的用户不付这份 IO，`DebugManager::new` 也因此保持同步。
//! 代价是「文件坏了、已挪开」的提示要等第一次访问才产生 —— 前端总是先读工作区再取提示，不会漏。
//!
//! 调用历史不在调用返回的路径上写：追加可能碰上压缩重写或回文件取全文，要排在存储的写闸后面。
//! 调用结束时只把记录丢进一个队列，由一个写入任务按调用结束的顺序逐条追加。写入任务只握着
//! 存储本身，管理器被丢弃、队列的发送端随之消失后它把剩下的写完就退出。
//!
//! 查历史和清空历史要先等队列里已有的写完：刚调完就去看历史的人应该看得到这一条，
//! 清空之后也不该有排着队的旧记录再冒出来。等待只看两个计数，队列是空的时候不付任何代价。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use ncd_domain::onebot_debug::{
    DebugCollections, DebugCollectionsSnapshot, DebugHistoryEntry, DebugHistoryPage,
    DebugHistoryQuery, DebugStorageNotice, DebugWorkspace, DebugWorkspaceSnapshot,
};
use tokio::sync::{Mutex, OnceCell, mpsc, watch};
use tracing::warn;

use super::DebugManager;
use super::storage::{DIR_NAME, DebugStore};

/// 排队等写的历史条数上限。正常情况下写入远快于人点「发送」，满了说明磁盘卡住了
const HISTORY_QUEUE_CAP: usize = 256;
/// 查历史前等排队写入的最长时间。磁盘真卡住了就先给出已有的，不让界面一直转
const DRAIN_WAIT: Duration = Duration::from_secs(2);

/// 惰性加载的存储
pub(super) struct LazyStore {
    data_root: PathBuf,
    cell: OnceCell<DebugStore>,
    initialization_gate: Mutex<()>,
    revision: AtomicU32,
}

impl LazyStore {
    pub(super) fn new(data_root: PathBuf) -> Self {
        Self {
            data_root,
            cell: OnceCell::new(),
            initialization_gate: Mutex::new(()),
            revision: AtomicU32::new(0),
        }
    }

    async fn get(&self) -> &DebugStore {
        if let Some(store) = self.cell.get() {
            return store;
        }
        let _gate = self.initialization_gate.lock().await;
        self.cell
            .get_or_init(|| DebugStore::load(&self.data_root))
            .await
    }

    async fn replace_config_with<R>(
        &self,
        workspace: Option<DebugWorkspace>,
        collections: Option<DebugCollections>,
        write: impl std::future::Future<Output = Result<R, String>>,
    ) -> Result<R, String> {
        if workspace.is_none() && collections.is_none() {
            return write.await;
        }
        let _gate = self.initialization_gate.lock().await;
        let result = match self.cell.get() {
            Some(store) => {
                store
                    .replace_config_with(workspace, collections, write)
                    .await
            }
            // Import must not load or quarantine existing files. The first access reads the commit.
            None => write.await,
        }?;
        self.revision.fetch_add(1, Ordering::SeqCst);
        Ok(result)
    }

    async fn workspace_snapshot(&self) -> DebugWorkspaceSnapshot {
        let _gate = self.initialization_gate.lock().await;
        let store = self
            .cell
            .get_or_init(|| DebugStore::load(&self.data_root))
            .await;
        DebugWorkspaceSnapshot {
            workspace: store.workspace().await,
            revision: self.revision.load(Ordering::SeqCst),
        }
    }

    async fn collections_snapshot(&self) -> DebugCollectionsSnapshot {
        let _gate = self.initialization_gate.lock().await;
        let store = self
            .cell
            .get_or_init(|| DebugStore::load(&self.data_root))
            .await;
        DebugCollectionsSnapshot {
            collections: store.collections().await,
            revision: self.revision.load(Ordering::SeqCst),
        }
    }

    async fn save_workspace_checked(
        &self,
        workspace: DebugWorkspace,
        revision: u32,
    ) -> Result<(), String> {
        let _gate = self.initialization_gate.lock().await;
        self.check_revision(revision)?;
        self.cell
            .get_or_init(|| DebugStore::load(&self.data_root))
            .await
            .save_workspace(workspace)
            .await
    }

    async fn save_collections_checked(
        &self,
        collections: DebugCollections,
        revision: u32,
    ) -> Result<(), String> {
        let _gate = self.initialization_gate.lock().await;
        self.check_revision(revision)?;
        self.cell
            .get_or_init(|| DebugStore::load(&self.data_root))
            .await
            .save_collections(collections)
            .await
    }

    fn check_revision(&self, revision: u32) -> Result<(), String> {
        if revision != self.revision.load(Ordering::SeqCst) {
            return Err("调试配置已从备份恢复，旧保存请求已取消，请刷新后重试".into());
        }
        Ok(())
    }

    /// 分块下载收拢出来的文件放这；调试台的落盘数据继续走 DebugStore，下载物只是临时文件
    pub(super) fn downloads_dir(&self) -> PathBuf {
        self.data_root.join(DIR_NAME).join("downloads")
    }
}

/// 历史写入队列；第一次记历史时才建队列和写入任务
pub(super) struct HistoryQueue {
    tx: OnceLock<mpsc::Sender<DebugHistoryEntry>>,
    /// 累计排进队列的条数
    enqueued: AtomicU64,
    /// 累计处理完的条数（写成功和写失败都算）。写入任务每处理一条加一
    done: Arc<watch::Sender<u64>>,
}

impl Default for HistoryQueue {
    fn default() -> Self {
        Self {
            tx: OnceLock::new(),
            enqueued: AtomicU64::new(0),
            done: Arc::new(watch::channel(0).0),
        }
    }
}

impl HistoryQueue {
    fn push(&self, store: &Arc<LazyStore>, entry: DebugHistoryEntry) -> bool {
        let tx = self.tx.get_or_init(|| {
            let (tx, rx) = mpsc::channel(HISTORY_QUEUE_CAP);
            tokio::spawn(write_history(Arc::clone(store), rx, Arc::clone(&self.done)));
            tx
        });
        let queued = tx.try_send(entry).is_ok();
        if queued {
            self.enqueued.fetch_add(1, Ordering::SeqCst);
        }
        queued
    }

    /// 等到调用这一刻之前排进来的都处理完，最多等 [`DRAIN_WAIT`]。不持任何锁
    async fn drained(&self) {
        let target = self.enqueued.load(Ordering::SeqCst);
        if *self.done.borrow() >= target {
            return;
        }
        let mut done = self.done.subscribe();
        if tokio::time::timeout(DRAIN_WAIT, done.wait_for(|n| *n >= target))
            .await
            .is_err()
        {
            warn!(
                "调试台调用历史写入 {} 秒还没写完，先按已写入的给出",
                DRAIN_WAIT.as_secs()
            );
        }
    }
}

async fn write_history(
    store: Arc<LazyStore>,
    mut entries: mpsc::Receiver<DebugHistoryEntry>,
    done: Arc<watch::Sender<u64>>,
) {
    while let Some(entry) = entries.recv().await {
        if let Err(e) = store.get().await.append_history(entry).await {
            warn!("记录调试台调用历史失败: {e}");
        }
        // 没人订阅时 send 会失败，send_modify 不管有没有人在等都会改值
        done.send_modify(|n| *n += 1);
    }
}

impl DebugManager {
    pub async fn workspace_snapshot(&self) -> DebugWorkspaceSnapshot {
        self.store.workspace_snapshot().await
    }

    pub async fn collections_snapshot(&self) -> DebugCollectionsSnapshot {
        self.store.collections_snapshot().await
    }

    pub async fn save_workspace_checked(
        &self,
        workspace: DebugWorkspace,
        revision: u32,
    ) -> Result<(), String> {
        self.store.save_workspace_checked(workspace, revision).await
    }

    pub async fn save_collections_checked(
        &self,
        collections: DebugCollections,
        revision: u32,
    ) -> Result<(), String> {
        self.store
            .save_collections_checked(collections, revision)
            .await
    }
    /// Commit imported workspace/collections without initializing storage or changing history.
    pub async fn replace_config_with<R>(
        &self,
        workspace: Option<DebugWorkspace>,
        collections: Option<DebugCollections>,
        write: impl std::future::Future<Output = Result<R, String>>,
    ) -> Result<R, String> {
        self.store
            .replace_config_with(workspace, collections, write)
            .await
    }

    pub async fn workspace(&self) -> DebugWorkspace {
        self.store.get().await.workspace().await
    }

    pub async fn save_workspace(&self, workspace: DebugWorkspace) -> Result<(), String> {
        self.store.get().await.save_workspace(workspace).await
    }

    pub async fn collections(&self) -> DebugCollections {
        self.store.get().await.collections().await
    }

    pub async fn save_collections(&self, collections: DebugCollections) -> Result<(), String> {
        self.store.get().await.save_collections(collections).await
    }

    pub async fn export_collections(&self, path: &Path) -> Result<(), String> {
        self.store.get().await.export_collections(path).await
    }

    /// 合并导入，返回合并后的收藏夹
    pub async fn import_collections(&self, path: &Path) -> Result<DebugCollections, String> {
        self.store.get().await.import_collections(path).await
    }

    /// 先等已排队的历史写完，刚结束的调用一定查得到
    pub async fn history(&self, query: DebugHistoryQuery) -> DebugHistoryPage {
        self.history_queue.drained().await;
        self.store.get().await.history(query).await
    }

    pub async fn history_entry(&self, id: &str) -> Option<DebugHistoryEntry> {
        self.store.get().await.history_entry(id).await
    }

    /// 先等已排队的历史写完再清，排着队的旧记录不会在清空之后又冒出来
    pub async fn clear_history(&self) -> Result<(), String> {
        self.history_queue.drained().await;
        self.store.get().await.clear_history().await
    }

    /// 取走读盘时积下的「文件坏了、已挪开」提示。
    /// 存储是惰性加载的，先等这次加载完再取 —— 不然第一条调用必然拿到空的（前端也有 gating 兜底）
    pub async fn take_storage_notices(&self) -> Vec<DebugStorageNotice> {
        self.store.get().await.take_storage_notices()
    }

    /// 把一条历史排进写入队列，立即返回
    pub(super) fn queue_history(&self, entry: DebugHistoryEntry) {
        if !self.history_queue.push(&self.store, entry) {
            warn!("调试台调用历史写入排队已满，丢掉这一条");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_traits::{ConfigStore, JsonTransaction};

    #[tokio::test]
    async fn old_workspace_save_queued_behind_import_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let store = Arc::new(LazyStore::new(root.path().to_path_buf()));
        let original = store.workspace_snapshot().await;
        let imported = DebugWorkspace {
            selected_bot: Some("imported".into()),
            ..Default::default()
        };
        let importing = Arc::clone(&store);
        let config = ncd_config::store::LocalConfigStore::new(root.path());
        let txn = JsonTransaction::new().write(
            root.path().join(DIR_NAME).join("workspace.json"),
            serde_json::to_value(&imported).unwrap(),
        );
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let import = tokio::spawn(async move {
            importing
                .replace_config_with(Some(imported), None, async {
                    entered_tx.send(()).unwrap();
                    release_rx.await.unwrap();
                    config.apply_transaction(txn).map_err(|e| e.to_string())
                })
                .await
        });
        entered_rx.await.unwrap();
        let saving = Arc::clone(&store);
        let mut save = tokio::spawn(async move {
            saving
                .save_workspace_checked(original.workspace, original.revision)
                .await
        });
        assert!(
            tokio::time::timeout(Duration::from_millis(30), &mut save)
                .await
                .is_err()
        );
        release_tx.send(()).unwrap();
        import.await.unwrap().unwrap();
        assert!(save.await.unwrap().is_err());
        assert_eq!(
            store
                .workspace_snapshot()
                .await
                .workspace
                .selected_bot
                .as_deref(),
            Some("imported")
        );
    }

    #[tokio::test]
    async fn old_collections_snapshot_cannot_overwrite_an_imported_collection() {
        let root = tempfile::tempdir().unwrap();
        let store = LazyStore::new(root.path().to_path_buf());
        let original = store.collections_snapshot().await;
        let imported: DebugCollections = serde_json::from_value(serde_json::json!({
            "version": 1, "folders": [{ "id": "restored", "name": "恢复的收藏", "order": 0 }], "requests": []
        })).unwrap();
        let txn = JsonTransaction::new().write(
            root.path().join(DIR_NAME).join("collections.json"),
            serde_json::to_value(&imported).unwrap(),
        );
        store
            .replace_config_with(None, Some(imported.clone()), async {
                ncd_config::store::LocalConfigStore::new(root.path())
                    .apply_transaction(txn)
                    .map_err(|e| e.to_string())
            })
            .await
            .unwrap();
        assert!(
            store
                .save_collections_checked(original.collections, original.revision)
                .await
                .is_err()
        );
        assert_eq!(store.collections_snapshot().await.collections, imported);
    }

    #[tokio::test]
    async fn importing_unloaded_debug_config_does_not_initialize_or_quarantine_files() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join(DIR_NAME);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("workspace.json"), b"corrupt workspace").unwrap();
        let store = LazyStore::new(root.path().to_path_buf());
        let workspace = DebugWorkspace {
            selected_bot: Some("imported".into()),
            ..Default::default()
        };
        let transaction = JsonTransaction::new().write(
            dir.join("workspace.json"),
            serde_json::to_value(&workspace).unwrap(),
        );
        store
            .replace_config_with(Some(workspace.clone()), None, async {
                ncd_config::store::LocalConfigStore::new(root.path())
                    .apply_transaction(transaction)
                    .map_err(|e| e.to_string())
            })
            .await
            .unwrap();
        assert!(store.cell.get().is_none());
        assert!(std::fs::read_dir(&dir).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("broken-")
        }));
        assert_eq!(store.get().await.workspace().await, workspace);
        assert!(store.get().await.take_storage_notices().is_empty());
    }

    #[tokio::test]
    async fn failed_import_leaves_unloaded_corrupt_debug_files_in_place() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join(DIR_NAME);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("workspace.json");
        std::fs::write(&path, b"corrupt workspace").unwrap();
        let store = LazyStore::new(root.path().to_path_buf());
        assert!(
            store
                .replace_config_with(Some(DebugWorkspace::default()), None, async {
                    Err::<(), _>("transaction failed".to_owned())
                })
                .await
                .is_err()
        );
        assert!(store.cell.get().is_none());
        assert_eq!(std::fs::read(&path).unwrap(), b"corrupt workspace");
    }

    #[tokio::test]
    async fn debug_initialization_waits_for_the_import_commit() {
        let root = tempfile::tempdir().unwrap();
        let config = ncd_config::store::LocalConfigStore::new(root.path());
        let path = root.path().join(DIR_NAME).join("workspace.json");
        config
            .write_json_atomic(
                &path,
                &serde_json::to_value(DebugWorkspace {
                    selected_bot: Some("old".into()),
                    ..Default::default()
                })
                .unwrap(),
            )
            .unwrap();
        let store = Arc::new(LazyStore::new(root.path().to_path_buf()));
        let workspace = DebugWorkspace {
            selected_bot: Some("imported".into()),
            ..Default::default()
        };
        let transaction =
            JsonTransaction::new().write(path, serde_json::to_value(&workspace).unwrap());
        let importing = Arc::clone(&store);
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let import = tokio::spawn(async move {
            importing
                .replace_config_with(Some(workspace), None, async {
                    entered_tx.send(()).unwrap();
                    release_rx.await.unwrap();
                    config
                        .apply_transaction(transaction)
                        .map_err(|e| e.to_string())
                })
                .await
        });
        entered_rx.await.unwrap();
        let reading = Arc::clone(&store);
        let mut reader = tokio::spawn(async move { reading.get().await.workspace().await });
        assert!(
            tokio::time::timeout(Duration::from_millis(30), &mut reader)
                .await
                .is_err()
        );
        release_tx.send(()).unwrap();
        import.await.unwrap().unwrap();
        assert_eq!(
            reader.await.unwrap().selected_bot.as_deref(),
            Some("imported")
        );
    }
}
