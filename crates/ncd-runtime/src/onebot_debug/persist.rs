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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use ncd_domain::onebot_debug::{
    DebugCollections, DebugHistoryEntry, DebugHistoryPage, DebugHistoryQuery, DebugStorageNotice,
    DebugWorkspace,
};
use tokio::sync::{OnceCell, mpsc, watch};
use tracing::warn;

use super::DebugManager;
use super::storage::DebugStore;

/// 排队等写的历史条数上限。正常情况下写入远快于人点「发送」，满了说明磁盘卡住了
const HISTORY_QUEUE_CAP: usize = 256;
/// 查历史前等排队写入的最长时间。磁盘真卡住了就先给出已有的，不让界面一直转
const DRAIN_WAIT: Duration = Duration::from_secs(2);

/// 惰性加载的存储
pub(super) struct LazyStore {
    data_root: PathBuf,
    cell: OnceCell<DebugStore>,
}

impl LazyStore {
    pub(super) fn new(data_root: PathBuf) -> Self {
        Self {
            data_root,
            cell: OnceCell::new(),
        }
    }

    async fn get(&self) -> &DebugStore {
        self.cell
            .get_or_init(|| DebugStore::load(&self.data_root))
            .await
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
