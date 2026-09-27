//! 服务器状态条：一次 exec 把 /proc 里要的几样和根分区读回来
//!
//! CPU 占用和网速要跟上一次读数相减，上一次的样本按主机记在会话管理器里；第一次读这两项为空。
//! 同一台主机开了好几个标签也只记一份，谁读都是和这台机上一次比。

use std::time::{Duration, Instant};

use ncd_domain::{ServerStats, TerminalHostOs};
use ncd_host::HostCommand;

use super::TerminalError;
use super::manager::TerminalManager;

/// 上一次的原始读数
pub(crate) struct StatsSample {
    at: Instant,
    cpu_total: u64,
    cpu_idle: u64,
    net_rx: u64,
    net_tx: u64,
}

const STATS_SCRIPT: &str = "cat /proc/loadavg; echo @@; cat /proc/meminfo; echo @@; \
head -n1 /proc/stat; echo @@; cat /proc/net/dev; echo @@; df -kP / | tail -n1; echo @@; \
cat /proc/uptime; echo @@; nproc 2>/dev/null || grep -c ^processor /proc/cpuinfo";

#[derive(Debug, Clone, PartialEq)]
struct RawStats {
    load: (f32, f32, f32),
    mem_total_kb: u64,
    mem_available_kb: u64,
    swap_total_kb: u64,
    swap_free_kb: u64,
    cpu_total: u64,
    cpu_idle: u64,
    net_rx: u64,
    net_tx: u64,
    disk_total_kb: u64,
    disk_used_kb: u64,
    uptime_secs: u64,
    cores: u32,
}

fn meminfo_kb(text: &str, key: &str) -> Option<u64> {
    text.lines()
        .find_map(|line| line.strip_prefix(key)?.strip_prefix(':'))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|n| n.parse().ok())
}

fn parse_stats(stdout: &str) -> Option<RawStats> {
    let parts: Vec<&str> = stdout.split("@@").map(str::trim).collect();
    if parts.len() < 7 {
        return None;
    }
    let mut load = parts[0].split_whitespace().map(|n| n.parse::<f32>().unwrap_or(0.0));
    let load = (
        load.next().unwrap_or(0.0),
        load.next().unwrap_or(0.0),
        load.next().unwrap_or(0.0),
    );

    let mem = parts[1];
    let mem_total_kb = meminfo_kb(mem, "MemTotal")?;
    // 老内核没有 MemAvailable，用空闲 + 缓存凑
    let mem_available_kb = meminfo_kb(mem, "MemAvailable").unwrap_or_else(|| {
        meminfo_kb(mem, "MemFree").unwrap_or(0)
            + meminfo_kb(mem, "Buffers").unwrap_or(0)
            + meminfo_kb(mem, "Cached").unwrap_or(0)
    });
    let swap_total_kb = meminfo_kb(mem, "SwapTotal").unwrap_or(0);
    let swap_free_kb = meminfo_kb(mem, "SwapFree").unwrap_or(0);

    // cpu  user nice system idle iowait irq softirq steal ...
    let cpu: Vec<u64> = parts[2]
        .split_whitespace()
        .skip(1)
        .take(8)
        .map(|n| n.parse().unwrap_or(0))
        .collect();
    let cpu_total = cpu.iter().sum();
    let cpu_idle = cpu.get(3).copied().unwrap_or(0) + cpu.get(4).copied().unwrap_or(0);

    let (mut net_rx, mut net_tx) = (0u64, 0u64);
    for line in parts[3].lines() {
        let Some((iface, rest)) = line.split_once(':') else {
            continue;
        };
        if iface.trim() == "lo" {
            continue;
        }
        let fields: Vec<u64> = rest
            .split_whitespace()
            .map(|n| n.parse().unwrap_or(0))
            .collect();
        net_rx += fields.first().copied().unwrap_or(0);
        net_tx += fields.get(8).copied().unwrap_or(0);
    }

    let disk: Vec<&str> = parts[4].split_whitespace().collect();
    let disk_total_kb = disk.get(1).and_then(|n| n.parse().ok()).unwrap_or(0);
    let disk_used_kb = disk.get(2).and_then(|n| n.parse().ok()).unwrap_or(0);

    let uptime_secs = parts[5]
        .split_whitespace()
        .next()
        .and_then(|n| n.parse::<f64>().ok())
        .map_or(0, |s| s as u64);
    let cores = parts[6].lines().next().and_then(|n| n.trim().parse().ok()).unwrap_or(1);

    Some(RawStats {
        load,
        mem_total_kb,
        mem_available_kb,
        swap_total_kb,
        swap_free_kb,
        cpu_total,
        cpu_idle,
        net_rx,
        net_tx,
        disk_total_kb,
        disk_used_kb,
        uptime_secs,
        cores,
    })
}

fn build_stats(raw: &RawStats, previous: Option<&StatsSample>, now: Instant) -> ServerStats {
    let (cpu_percent, net_rx_per_sec, net_tx_per_sec) = match previous {
        Some(prev) => {
            let total = raw.cpu_total.saturating_sub(prev.cpu_total);
            let idle = raw.cpu_idle.saturating_sub(prev.cpu_idle);
            let cpu = (total > 0).then(|| (total.saturating_sub(idle)) as f32 / total as f32 * 100.0);
            let secs = now.duration_since(prev.at).as_secs_f64();
            let rate = |cur: u64, old: u64| {
                (secs > 0.0).then(|| (cur.saturating_sub(old) as f64 / secs) as u64)
            };
            (cpu, rate(raw.net_rx, prev.net_rx), rate(raw.net_tx, prev.net_tx))
        }
        None => (None, None, None),
    };
    ServerStats {
        cpu_percent,
        cores: raw.cores,
        mem_used: raw.mem_total_kb.saturating_sub(raw.mem_available_kb) * 1024,
        mem_total: raw.mem_total_kb * 1024,
        swap_used: raw.swap_total_kb.saturating_sub(raw.swap_free_kb) * 1024,
        swap_total: raw.swap_total_kb * 1024,
        disk_used: raw.disk_used_kb * 1024,
        disk_total: raw.disk_total_kb * 1024,
        net_rx_per_sec,
        net_tx_per_sec,
        load1: raw.load.0,
        load5: raw.load.1,
        load15: raw.load.2,
        uptime_secs: raw.uptime_secs,
    }
}

impl TerminalManager {
    pub async fn host_stats(&self, id: &str) -> Result<ServerStats, TerminalError> {
        let (host, os, host_id) = self.session_host(id)?;
        if os != TerminalHostOs::Linux {
            return Err(TerminalError::Unsupported("只有远端 Linux 主机有状态条".into()));
        }
        let out = host
            .run_to_string(
                HostCommand::new("sh")
                    .arg("-c")
                    .arg(STATS_SCRIPT)
                    .timeout(Duration::from_secs(8)),
            )
            .await
            .map_err(|e| TerminalError::Host(format!("读服务器状态失败：{e}")))?;
        let raw = parse_stats(&out.stdout)
            .ok_or_else(|| TerminalError::Host("服务器状态读出来的格式不认识".into()))?;
        let now = Instant::now();
        let mut samples = self.stats.lock().await;
        let stats = build_stats(&raw, samples.get(&host_id), now);
        samples.insert(
            host_id,
            StatsSample {
                at: now,
                cpu_total: raw.cpu_total,
                cpu_idle: raw.cpu_idle,
                net_rx: raw.net_rx,
                net_tx: raw.net_tx,
            },
        );
        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "0.52 0.40 0.30 1/234 5678
@@
MemTotal:        4000000 kB
MemFree:          500000 kB
MemAvailable:    3000000 kB
Buffers:          100000 kB
Cached:           900000 kB
SwapTotal:       1000000 kB
SwapFree:         750000 kB
@@
cpu  100 0 50 800 50 0 0 0 0 0
@@
Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo: 999999 10 0 0 0 0 0 0 999999 10 0 0 0 0 0 0
  eth0: 1000 10 0 0 0 0 0 0 2000 20 0 0 0 0 0 0
@@
/dev/vda1         41152832 12345678  26700000      32% /
@@
86400.55 123456.00
@@
2
";

    #[test]
    fn parses_proc_snapshot() {
        let raw = parse_stats(SAMPLE).unwrap();
        assert_eq!(raw.load, (0.52, 0.40, 0.30));
        assert_eq!(raw.mem_total_kb, 4_000_000);
        assert_eq!(raw.mem_available_kb, 3_000_000);
        assert_eq!(raw.cpu_total, 1000);
        assert_eq!(raw.cpu_idle, 850);
        assert_eq!((raw.net_rx, raw.net_tx), (1000, 2000));
        assert_eq!((raw.disk_total_kb, raw.disk_used_kb), (41_152_832, 12_345_678));
        assert_eq!(raw.uptime_secs, 86_400);
        assert_eq!(raw.cores, 2);
    }

    #[test]
    fn rates_need_a_previous_sample() {
        let raw = parse_stats(SAMPLE).unwrap();
        let now = Instant::now();
        let first = build_stats(&raw, None, now);
        assert_eq!(first.cpu_percent, None);
        assert_eq!(first.net_rx_per_sec, None);
        assert_eq!(first.mem_used, 1_000_000 * 1024);
        assert_eq!(first.swap_used, 250_000 * 1024);

        let prev = StatsSample {
            at: now - Duration::from_secs(2),
            cpu_total: 800,
            cpu_idle: 750,
            net_rx: 0,
            net_tx: 1000,
        };
        let second = build_stats(&raw, Some(&prev), now);
        // 200 个时间片里空闲 100，占用一半
        assert_eq!(second.cpu_percent, Some(50.0));
        assert_eq!(second.net_rx_per_sec, Some(500));
        assert_eq!(second.net_tx_per_sec, Some(500));
    }

    #[test]
    fn old_kernel_without_mem_available_is_estimated() {
        let text = SAMPLE.replace("MemAvailable:    3000000 kB\n", "");
        let raw = parse_stats(&text).unwrap();
        assert_eq!(raw.mem_available_kb, 1_500_000);
    }
}
