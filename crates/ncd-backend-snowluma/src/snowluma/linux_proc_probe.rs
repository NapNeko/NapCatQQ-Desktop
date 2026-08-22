//! 远端 Linux QQ 进程树探测:UIN 锁定只需主进程 PID 在候选集合内

use std::collections::BTreeSet;

use async_trait::async_trait;

use crate::snowluma::status_poller::ProcessTreeProbe;

/// 远端 SnowLuma:无 Windows 式子进程枚举,仅把 bot 主 PID 当作候选集
#[derive(Debug, Clone, Copy)]
pub struct LinuxSinglePidProbe {
    pid: u32,
}

impl LinuxSinglePidProbe {
    pub fn new(pid: u32) -> Self {
        Self { pid }
    }
}

#[async_trait]
impl ProcessTreeProbe for LinuxSinglePidProbe {
    async fn collect_descendants(&self, _initial_pid: u32) -> BTreeSet<u32> {
        BTreeSet::from([self.pid])
    }
}
