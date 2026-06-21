//! 读取本机 CPU / 内存占用百分比,供概览性能监控曲线使用
//!
//! sysinfo 的 cpu_usage() 是「自上次 refresh 以来的增量」,必须在**同一 System
//! 实例**上按间隔连续 refresh_cpu_usage(),每次 IPC 新建 System 会得到 0 或乱跳

use std::sync::Mutex;
use std::time::{Duration, Instant};

use ncd_domain::SystemResourceSnapshot;
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

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

    /// force_cpu_ready:首屏 bootstrap 采样为 true,跳过 200ms 等待与暖机 sleep
    fn sample(&mut self, force_cpu_ready: bool) -> SystemResourceSnapshot {
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

        SystemResourceSnapshot {
            cpu_percent: cpu_percent.clamp(0.0, 100.0),
            ram_percent: ram_percent.clamp(0.0, 100.0),
        }
    }
}

static SAMPLER: Mutex<Option<MetricsSampler>> = Mutex::new(None);

#[tauri::command]
pub fn get_system_resource_snapshot(bootstrap: Option<bool>) -> Result<SystemResourceSnapshot, String> {
    let force = bootstrap.unwrap_or(false);
    let mut guard = SAMPLER
        .lock()
        .map_err(|_| "系统指标采样器锁失败".to_string())?;
    if guard.is_none() {
        *guard = Some(MetricsSampler::new());
    }
    let sampler = guard
        .as_mut()
        .ok_or_else(|| "系统指标采样器未初始化".to_string())?;
    Ok(sampler.sample(force))
}