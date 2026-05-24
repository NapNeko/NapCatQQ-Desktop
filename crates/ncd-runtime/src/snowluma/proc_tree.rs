//! `ProcessTreeProbe` 的真实系统实现 + 测试用 mock。
//!
//! `ProcessTreeProbe` trait 本体定义在
//! [`crate::snowluma::status_poller`] 中。
//! 本文件仅提供 `SysinfoProcessTreeProbe`（生产实装）与 `MockProcessTreeProbe`
//! （单元测试 helper），避免 trait 重复定义。
//! 设计要点（与 对齐）：
//! - SnowLuma 仅在 Windows 工作（`runtime_backend::start` 在非 Windows 上直接返回
//! `Unsupported`），但本 trait 实装跨平台编译可通过：非 Windows 上直接返回
//! 只含 `initial_pid` 的集合，避免在 macOS / Linux CI 上把测试拉爆。
//! - `sysinfo::System::new_all()` 在 Windows 上枚举所有进程开销可观（数十毫秒）
//! 必须放进 `tokio::task::spawn_blocking` 跑，否则会阻塞主 runtime 上的其它
//! I/O / 计时任务。
//! - 实现失败兜底：sysinfo 找不到 `initial_pid`、权限不足、API 调用 panic
//! 等所有异常情况都收敛到「返回单元素集合 `{initial_pid}`」，不向上抛。
//! Status poller 允许"暂时拿不到子进程"，下一轮会再 probe。

use std::collections::{BTreeSet, HashMap};

use async_trait::async_trait;

use crate::snowluma::status_poller::ProcessTreeProbe;

// ---------------------------------------------------------------------------
// Sysinfo 实装
// ---------------------------------------------------------------------------

/// 基于 `sysinfo` crate 的 `ProcessTreeProbe` 默认实装。
/// 无状态结构（`Copy`）：每次 `collect_descendants` 都重新构造一个
/// `sysinfo::System` 并 `new_all` 刷新。每轮 poller tick 之间不复用
/// 避免脏快照（process exit / new spawn）影响 BFS 结果。
/// # 平台差异
/// - Windows：通过 sysinfo 枚举所有进程，按 `parent` 链路 BFS。
/// - 非 Windows：直接返回 `{initial_pid}`。SnowLuma 不在非 Windows 上运行
/// 保留只是为了让 ncd-core 在非 Windows CI 上仍可 `cargo test --lib`。
#[derive(Debug, Default, Clone, Copy)]
pub struct SysinfoProcessTreeProbe;

impl SysinfoProcessTreeProbe {
    /// 构造无状态 probe。
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ProcessTreeProbe for SysinfoProcessTreeProbe {
    async fn collect_descendants(&self, initial_pid: u32) -> BTreeSet<u32> {
        // sysinfo 调用是同步阻塞的（new_all + 内部 win32 API）
        // 必须 offload 到 blocking pool 以免占用主 runtime worker。
        let join = tokio::task::spawn_blocking(move || collect_descendants_blocking(initial_pid));

        // spawn_blocking 几乎不会失败；万一 panic 也不该把整个 poller 拖垮
        // 收敛到「至少含 initial_pid 自身」的退化集合。
        match join.await {
            Ok(set) => set,
            Err(_) => BTreeSet::from([initial_pid]),
        }
    }
}

/// 同步实现入口：在阻塞 pool 中执行的纯计算。
/// 该函数对所有失败路径（sysinfo 拿不到进程、parent 链断裂、平台不支持）
/// 都返回 `{initial_pid}` 单元素集合，不向上传播错误。
/// `#[cfg(windows)]` 路径在非 Windows 平台被裁剪，只剩 fallback 分支。
#[cfg(windows)]
fn collect_descendants_blocking(initial_pid: u32) -> BTreeSet<u32> {
    // sysinfo 0.31：`System::new_all` 内部已经调用 `refresh_processes_specifics`
    // 拉满了 process snapshot，无须再额外 refresh。
    let system = sysinfo::System::new_all();
    let processes = system.processes();

    // 构造 parent -> children 邻接表，避免在 BFS 内对 N 个进程做 N 次扫描
    // (O(N^2) -> O(N))。Windows 上典型进程数量 200~500，列表分配廉价。
    let mut parent_to_children: HashMap<u32, Vec<u32>> = HashMap::new();
    for (pid, process) in processes.iter() {
        if let Some(parent_pid) = process.parent() {
            parent_to_children
                .entry(parent_pid.as_u32())
                .or_default()
                .push(pid.as_u32());
        }
    }

    // 起始 PID 必须存在于 sysinfo 快照中（否则视为已退出 / 拿不到
    // 直接返回单元素集合，下一轮 tick 再试）。
    let initial = sysinfo::Pid::from_u32(initial_pid);
    if !processes.contains_key(&initial) {
        return BTreeSet::from([initial_pid]);
    }

    // 标准 BFS：visited 既做去重又是返回结果。
    let mut visited: BTreeSet<u32> = BTreeSet::new();
    let mut queue: Vec<u32> = vec![initial_pid];
    while let Some(pid) = queue.pop() {
        if !visited.insert(pid) {
            continue;
        }
        if let Some(children) = parent_to_children.get(&pid) {
            for child in children {
                if !visited.contains(child) {
                    queue.push(*child);
                }
            }
        }
    }

    // 兜底：BFS 应至少含 initial_pid 自身（前面已 contains_key 检查）
    // 但保险起见显式插入一次以保证不变量。
    visited.insert(initial_pid);
    visited
}

#[cfg(not(windows))]
fn collect_descendants_blocking(initial_pid: u32) -> BTreeSet<u32> {
    // SnowLuma backend 在非 Windows 平台被 runtime_backend::start 早期拒绝
    // 这里仅保证 trait 编译可过，行为退化为单元素集合。
    BTreeSet::from([initial_pid])
}

// ---------------------------------------------------------------------------
// 单测 helper：MockProcessTreeProbe
// ---------------------------------------------------------------------------

/// 单元测试用的 `ProcessTreeProbe` mock。
/// 上层（`SnowLumaStatusPoller` / `SnowLumaRuntimeBackend`）测试通过它注入
/// 固定的"候选 PID 集合"快照，无需真正起进程。
/// 用法：
/// ```ignore
/// let mock = MockProcessTreeProbe::with_set([12345, 12346])
/// let probe: Arc<dyn ProcessTreeProbe> = Arc::new(mock)
/// ```
/// 内部用 `std::sync::Mutex<BTreeSet<u32>>`：trait 的 `collect_descendants`
/// 接收 `&self`，且测试期间允许跨 task / 线程读，因此选标准库 Mutex 而非
/// `tokio::sync::Mutex`（前者无 await 依赖、对小集合开销可忽略）。
#[derive(Debug, Default, Clone)]
pub struct MockProcessTreeProbe {
    result: std::sync::Arc<std::sync::Mutex<BTreeSet<u32>>>,
}

impl MockProcessTreeProbe {
    /// 用空集合构造；调用方可后续 `set` 注入。
    pub fn new() -> Self {
        Self::default()
    }

    /// 用一个 PID 集合直接构造。返回值会忽略 `collect_descendants` 的
    /// `initial_pid` 入参，按构造时给定的固定集合返回。
    pub fn with_set<I: IntoIterator<Item = u32>>(pids: I) -> Self {
        Self {
            result: std::sync::Arc::new(std::sync::Mutex::new(pids.into_iter().collect())),
        }
    }

    /// 替换内部固定集合（后续 `collect_descendants` 调用都返回新值）。
    pub fn set<I: IntoIterator<Item = u32>>(&self, pids: I) {
        if let Ok(mut guard) = self.result.lock() {
            *guard = pids.into_iter().collect();
        }
    }
}

#[async_trait]
impl ProcessTreeProbe for MockProcessTreeProbe {
    async fn collect_descendants(&self, _initial_pid: u32) -> BTreeSet<u32> {
        // 锁中毒（极少）：返回空集合而不是 panic，让上层走"未发现候选"分支。
        match self.result.lock() {
            Ok(guard) => guard.clone(),
            Err(_) => BTreeSet::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_returns_configured_set() {
        let probe = MockProcessTreeProbe::with_set([12345, 12346, 99999]);
        // initial_pid 入参对 mock 不影响返回值
        let result = probe.collect_descendants(12345).await;
        assert_eq!(result, BTreeSet::from([12345, 12346, 99999]));
    }

    #[tokio::test]
    async fn mock_set_replaces_previous_value() {
        let probe = MockProcessTreeProbe::with_set([1, 2]);
        assert_eq!(probe.collect_descendants(1).await, BTreeSet::from([1, 2]));

        probe.set([10, 20, 30]);
        assert_eq!(
            probe.collect_descendants(1).await,
            BTreeSet::from([10, 20, 30]),
        );
    }

    #[tokio::test]
    async fn mock_default_returns_empty_set() {
        let probe = MockProcessTreeProbe::new();
        assert!(probe.collect_descendants(12345).await.is_empty());
    }

    /// 非 Windows 平台：sysinfo 路径被裁剪，应该返回 `{initial_pid}`。
    /// 这一条同时保证非 Windows CI 上 trait 实装编译通过。
    #[cfg(not(windows))]
    #[tokio::test]
    async fn sysinfo_probe_non_windows_returns_singleton() {
        let probe = SysinfoProcessTreeProbe::new();
        let result = probe.collect_descendants(42).await;
        assert_eq!(result, BTreeSet::from([42]));
    }

    /// Windows smoke 测试：用一个几乎不可能存在的 PID（`u32::MAX`）调用真实
    /// sysinfo 路径。期望表现：
    /// - 不 panic（崩溃）
    /// - 至少返回 `{u32::MAX}`（fallback 单元素集合）
    /// 不验证集合内容，因为本机进程列表测试时不可控。
    #[cfg(windows)]
    #[tokio::test]
    async fn sysinfo_probe_windows_unknown_pid_returns_singleton() {
        let probe = SysinfoProcessTreeProbe::new();
        let result = probe.collect_descendants(u32::MAX).await;
        assert!(result.contains(&u32::MAX));
        // PID = u32::MAX 不可能命中真实进程，结果集应仅含自身
        assert_eq!(result.len(), 1);
    }

    /// Windows smoke 测试：用当前进程 PID 调用 sysinfo 路径。
    /// 期望至少返回包含自身 PID 的非空集合，不验证 children 数量
    /// （test runner 自身可能起 / 不起 worker 子进程，不可控）。
    #[cfg(windows)]
    #[tokio::test]
    async fn sysinfo_probe_windows_current_pid_includes_self() {
        let probe = SysinfoProcessTreeProbe::new();
        let pid = std::process::id();
        let result = probe.collect_descendants(pid).await;
        assert!(
            result.contains(&pid),
            "结果集合必须至少含自身 PID {pid}, got {result:?}"
        );
    }
}
