// 远端终端底下的一行服务器状态：CPU / 内存 / 磁盘 / 网速 / 负载 / 开机时长。

import { Cpu, HardDrive, MemoryStick, Timer, Activity, ArrowDown, ArrowUp } from 'lucide-react';
import { cn } from '../../shared/utils/cn';
import { useTerminalStats } from '../../hooks/terminal/useTerminalStats';
import {
    formatBytes,
    formatRate,
    formatUptime,
    loadTone,
    percent,
} from '../../core/domain/terminal/format';

function Meter({ value }: { value: number }) {
    const tone = loadTone(value);
    return (
        <span className="relative inline-block h-1.5 w-10 overflow-hidden rounded-pill bg-border-subtle">
            <span
                className={cn(
                    'absolute inset-y-0 left-0 rounded-pill transition-[width] duration-500',
                    tone === 'danger' ? 'bg-danger' : tone === 'warn' ? 'bg-warning' : 'bg-success',
                )}
                style={{ width: `${Math.max(2, value)}%` }}
            />
        </span>
    );
}

export function TerminalStatsBar({ sessionId, visible }: { sessionId: string; visible: boolean }) {
    const { stats, stale } = useTerminalStats(sessionId, visible);
    if (!stats) {
        return (
            <div className="flex h-6 shrink-0 items-center border-t border-border-subtle px-3 text-[11px] text-text-tertiary">
                正在读服务器状态…
            </div>
        );
    }
    const mem = percent(stats.mem_used, stats.mem_total);
    const disk = percent(stats.disk_used, stats.disk_total);
    const cpu = stats.cpu_percent === undefined ? '—' : `${stats.cpu_percent.toFixed(0)}%`;
    const memText = `${formatBytes(stats.mem_used)} / ${formatBytes(stats.mem_total)}`;
    const diskText = `${formatBytes(stats.disk_used)} / ${formatBytes(stats.disk_total)}`;
    const load = `${stats.load1.toFixed(2)} ${stats.load5.toFixed(2)} ${stats.load15.toFixed(2)}`;
    const uptime = formatUptime(stats.uptime_secs);
    // 窄的时候（分屏）后面几项会藏起来，悬停整条能看全
    const summary = [
        stale ? '最近一次没读到，显示的是上一次的数' : null,
        `CPU ${cpu}（${stats.cores} 核）`,
        `内存 ${memText}`,
        stats.swap_total
            ? `交换 ${formatBytes(stats.swap_used)} / ${formatBytes(stats.swap_total)}`
            : null,
        `根分区 ${diskText}`,
        `网速 ↓ ${formatRate(stats.net_rx_per_sec)} ↑ ${formatRate(stats.net_tx_per_sec)}`,
        `负载（1 / 5 / 15 分钟）${load}`,
        `已开机 ${uptime}`,
    ]
        .filter(Boolean)
        .join('\n');
    return (
        <div className="@container shrink-0 border-t border-border-subtle" title={summary}>
            <div
                className={cn(
                    'flex h-6 items-center gap-4 overflow-hidden whitespace-nowrap px-3 text-[11px] tabular-nums text-text-secondary',
                    stale && 'opacity-60',
                )}
            >
                <span className="flex items-center gap-1.5">
                    <Cpu size={12} className="text-text-tertiary" />
                    {cpu}
                    {stats.cpu_percent !== undefined && <Meter value={stats.cpu_percent} />}
                </span>
                <span className="flex items-center gap-1.5">
                    <MemoryStick size={12} className="text-text-tertiary" />
                    {memText}
                    <Meter value={mem} />
                </span>
                <span className="hidden items-center gap-1.5 @min-[27rem]:flex">
                    <HardDrive size={12} className="text-text-tertiary" />
                    {diskText}
                    <Meter value={disk} />
                </span>
                <span className="hidden items-center gap-1 @min-[37rem]:flex">
                    <ArrowDown size={12} className="text-text-tertiary" />
                    {formatRate(stats.net_rx_per_sec)}
                    <ArrowUp size={12} className="ml-1 text-text-tertiary" />
                    {formatRate(stats.net_tx_per_sec)}
                </span>
                <span className="hidden items-center gap-1 @min-[44rem]:flex">
                    <Activity size={12} className="text-text-tertiary" />
                    {load}
                </span>
                <span className="hidden items-center gap-1 @min-[50rem]:flex">
                    <Timer size={12} className="text-text-tertiary" />
                    {uptime}
                </span>
            </div>
        </div>
    );
}
