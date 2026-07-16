//! 读取本机 CPU / 内存,供概览性能曲线与 Bot 实例指标「主机资源」合并
//!
//! sysinfo 的 cpu_usage() 是「自上次 refresh 以来的增量」,必须在**同一 System
//! 实例**上按间隔连续 refresh_cpu_usage(),每次 IPC 新建 System 会得到 0 或乱跳

use std::sync::Mutex;
use std::time::{Duration, Instant};

use ncd_domain::SystemResourceSnapshot;
use sysinfo::{CpuRefreshKind, Disks, MemoryRefreshKind, RefreshKind, System};

/// 本机主机资源采样（百分比 + 绝对值）
#[derive(Debug, Clone, Copy)]
pub struct HostResourceSample {
    pub cpu_percent: f64,
    pub ram_percent: f64,
    pub total_memory_bytes: u64,
    pub used_memory_bytes: u64,
    pub disk_total_bytes: u64,
    pub disk_used_bytes: u64,
}

struct MetricsSampler {
    system: System,
    last_cpu_refresh: Instant,
    cpu_warmed: bool,
}

impl MetricsSampler {
    fn new() -> Self {
        let mut system = System::new_with_specifics(
            RefreshKind::new()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        );
        system.refresh_cpu_usage();
        system.refresh_memory();
        Self {
            system,
            last_cpu_refresh: Instant::now(),
            cpu_warmed: false,
        }
    }

    /// force_cpu_ready:首屏 bootstrap / 列表批量 merge 为 true,跳过 200ms 等待与暖机 sleep
    fn sample(&mut self, force_cpu_ready: bool) -> HostResourceSample {
        if !force_cpu_ready {
            let elapsed = self.last_cpu_refresh.elapsed();
            if elapsed < Duration::from_millis(200) {
                std::thread::sleep(Duration::from_millis(200) - elapsed);
            }
        }
        self.system.refresh_cpu_usage();
        self.last_cpu_refresh = Instant::now();
        self.system.refresh_memory();

        let cpus = self.system.cpus();
        let cpu_count = cpus.len();
        let mut cpu_percent = if cpu_count == 0 {
            0.0
        } else {
            cpus.iter().map(|c| c.cpu_usage() as f64).sum::<f64>() / cpu_count as f64
        };

        if !force_cpu_ready && !self.cpu_warmed && cpu_percent < 0.5 {
            std::thread::sleep(Duration::from_millis(80));
            self.system.refresh_cpu_usage();
            self.last_cpu_refresh = Instant::now();
            if cpu_count > 0 {
                cpu_percent = self
                    .system
                    .cpus()
                    .iter()
                    .map(|c| c.cpu_usage() as f64)
                    .sum::<f64>()
                    / cpu_count as f64;
            }
        }
        self.cpu_warmed = true;

        let total = self.system.total_memory();
        let used = self.system.used_memory();
        let ram_percent = if total == 0 {
            0.0
        } else {
            (used as f64 / total as f64) * 100.0
        };

        let (disk_total, disk_used) = sample_system_disk();

        HostResourceSample {
            cpu_percent: cpu_percent.clamp(0.0, 100.0),
            ram_percent: ram_percent.clamp(0.0, 100.0),
            total_memory_bytes: total,
            used_memory_bytes: used,
            disk_total_bytes: disk_total,
            disk_used_bytes: disk_used,
        }
    }
}

/// 系统盘：Windows 优先 C:\，其它平台优先 /；找不到则取容量最大的盘
fn sample_system_disk() -> (u64, u64) {
    let disks = Disks::new_with_refreshed_list();
    let list = disks.list();
    if list.is_empty() {
        return (0, 0);
    }

    let preferred = list.iter().find(|d| {
        let mp = d.mount_point().to_string_lossy();
        #[cfg(windows)]
        {
            mp.eq_ignore_ascii_case("C:\\") || mp.eq_ignore_ascii_case("C:/")
        }
        #[cfg(not(windows))]
        {
            mp == "/"
        }
    });

    let disk = preferred.or_else(|| list.iter().max_by_key(|d| d.total_space()));
    match disk {
        Some(d) => {
            let total = d.total_space();
            let avail = d.available_space();
            let used = total.saturating_sub(avail);
            (total, used)
        }
        None => (0, 0),
    }
}

static SAMPLER: Mutex<Option<MetricsSampler>> = Mutex::new(None);

/// 供 bot_metrics 合并主机资源；force=true 避免列表路径 sleep
pub fn sample_host_resources(force_cpu_ready: bool) -> Result<HostResourceSample, String> {
    let mut guard = SAMPLER
        .lock()
        .map_err(|_| "系统指标采样器锁失败".to_string())?;
    if guard.is_none() {
        *guard = Some(MetricsSampler::new());
    }
    let sampler = guard
        .as_mut()
        .ok_or_else(|| "系统指标采样器未初始化".to_string())?;
    Ok(sampler.sample(force_cpu_ready))
}

#[tauri::command]
pub fn get_system_resource_snapshot(
    bootstrap: Option<bool>,
) -> Result<SystemResourceSnapshot, String> {
    let force = bootstrap.unwrap_or(false);
    let s = sample_host_resources(force)?;
    Ok(SystemResourceSnapshot {
        cpu_percent: s.cpu_percent,
        ram_percent: s.ram_percent,
    })
}
