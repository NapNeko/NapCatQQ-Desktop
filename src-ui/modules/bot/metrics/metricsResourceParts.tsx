// 指标页资源子件：资源占用条、系统资源卡片、节点出入流量表
// 系统资源卡片把主机内存 / CPU / 磁盘的派生计算收在自己内部（经 core/domain 纯函数），
// 主页面只传 metrics，不再内联做比例算术。

import { Cpu, Database, HardDrive, MemoryStick } from 'lucide-react';
import type { BotRuntimeMetrics } from '../../../core/ipc/generated/domain/BotRuntimeMetrics';
import type { NetworkNodeMetrics } from '../../../core/ipc/generated/domain/NetworkNodeMetrics';
import {
    formatBytes,
    formatCompactCount,
    networkNodeKindLabel,
} from '../../../core/domain/bot/runtime-metrics-settings';
import {
    formatActivity,
    hostCpuPercentOf,
    hostDiskRatioOf,
    hostMemoryRatioOf,
    rssRatioOfHostTotal,
} from '../../../core/domain/bot/runtime-metrics-display';
import { cn } from '../../../shared/utils/cn';
import { Panel } from './metricsPanelParts';

export function ResourceMeter({
    icon: Icon,
    label,
    value,
    detail,
    ratio,
    unavailable,
}: {
    icon: typeof MemoryStick;
    label: string;
    value: string;
    detail?: string;
    ratio?: number | null;
    unavailable?: boolean;
}) {
    const pct =
        ratio != null && Number.isFinite(ratio) ? Math.max(0, Math.min(100, ratio * 100)) : null;

    return (
        <div
            className={cn(
                'flex min-h-0 flex-1 flex-col justify-center rounded-sm bg-inset/45 px-3 py-2.5',
                unavailable && 'opacity-75',
            )}
        >
            <div className="flex items-center gap-2">
                <span className="inline-flex h-7 w-7 items-center justify-center rounded-sm bg-surface/70 text-brand ring-1 ring-border-subtle">
                    <Icon aria-hidden size={14} strokeWidth={2.1} />
                </span>
                <div className="min-w-0 flex-1">
                    <div className="flex items-baseline justify-between gap-2">
                        <p className="text-2xs font-medium text-text-tertiary">{label}</p>
                        <p className="truncate font-mono text-sm font-semibold tabular-nums text-text">
                            {value}
                        </p>
                    </div>
                    {detail ? (
                        <p className="mt-0.5 truncate text-2xs text-text-tertiary">{detail}</p>
                    ) : null}
                </div>
            </div>
            <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-inset">
                {pct != null ? (
                    <div
                        className="h-full rounded-full bg-brand/80 transition-[width]"
                        style={{ width: `${pct}%` }}
                    />
                ) : (
                    <div className="h-full w-full rounded-full bg-border-subtle/60" />
                )}
            </div>
        </div>
    );
}

export function SystemResourceCard({
    metrics,
    rss,
    isRemote,
}: {
    metrics: BotRuntimeMetrics | null;
    rss: number | null;
    isRemote: boolean;
}) {
    const heap = metrics?.memory?.heap_used_bytes;
    const hostUsed = metrics?.memory?.host_used_bytes;
    const hostTotal = metrics?.memory?.host_total_bytes;
    const hostDiskUsed = metrics?.memory?.host_disk_used_bytes;
    const hostDiskTotal = metrics?.memory?.host_disk_total_bytes;
    const hostRatio = hostMemoryRatioOf(metrics);
    // 主机 CPU/磁盘：本机 system_metrics / 远端 ncd-watch host-stats，不是进程探针
    const hostCpu = hostCpuPercentOf(metrics);
    const hostDiskRatio = hostDiskRatioOf(metrics);
    const hostResourceHint = isRemote ? '远端主机（ncd-watch host-stats）' : '本机主机采样';

    return (
        <Panel
            title="系统资源"
            description={
                hostCpu != null || hostDiskRatio != null || hostRatio != null
                    ? `进程内存 + ${hostResourceHint}`
                    : isRemote
                      ? '进程内存可用；主机 CPU/磁盘需 ncd-watch 写 host-stats'
                      : '进程内存可用；主机资源来自本机采样'
            }
            className="min-h-0"
        >
            <div className="flex h-full min-h-0 flex-col gap-2">
                <ResourceMeter
                    icon={MemoryStick}
                    label="进程 RSS"
                    value={formatBytes(rss)}
                    detail={heap != null ? `堆 ${formatBytes(Number(heap))}` : undefined}
                    ratio={rssRatioOfHostTotal(metrics)}
                />
                <ResourceMeter
                    icon={HardDrive}
                    label="主机内存"
                    value={
                        hostUsed != null || hostTotal != null
                            ? `${formatBytes(hostUsed != null ? Number(hostUsed) : null)} / ${formatBytes(hostTotal != null ? Number(hostTotal) : null)}`
                            : '—'
                    }
                    detail={
                        hostRatio != null
                            ? `占用 ${(hostRatio * 100).toFixed(1)}% · ${hostResourceHint}`
                            : isRemote
                              ? '未读到 host-stats 内存（检查 ncd-watch）'
                              : '本机主机内存暂不可用'
                    }
                    ratio={hostRatio}
                    unavailable={hostRatio == null}
                />
                <ResourceMeter
                    icon={Cpu}
                    label="CPU"
                    value={hostCpu != null ? `${hostCpu.toFixed(1)}%` : '—'}
                    detail={
                        hostCpu != null
                            ? `整机占用 · ${hostResourceHint}`
                            : isRemote
                              ? '未读到 host-stats CPU（检查 ncd-watch）'
                              : '本机 CPU 暂不可用'
                    }
                    ratio={hostCpu != null ? hostCpu / 100 : null}
                    unavailable={hostCpu == null}
                />
                <ResourceMeter
                    icon={Database}
                    label="磁盘"
                    value={
                        hostDiskUsed != null || hostDiskTotal != null
                            ? `${formatBytes(hostDiskUsed != null ? Number(hostDiskUsed) : null)} / ${formatBytes(hostDiskTotal != null ? Number(hostDiskTotal) : null)}`
                            : '—'
                    }
                    detail={
                        hostDiskRatio != null
                            ? `占用 ${(hostDiskRatio * 100).toFixed(1)}% · ${hostResourceHint}`
                            : isRemote
                              ? '未读到 host-stats 磁盘（检查 ncd-watch）'
                              : '本机磁盘暂不可用'
                    }
                    ratio={hostDiskRatio}
                    unavailable={hostDiskRatio == null}
                />
            </div>
        </Panel>
    );
}

export function NodeRows({ nodes }: { nodes: NetworkNodeMetrics[] }) {
    if (nodes.length === 0) {
        return (
            <div className="flex h-full min-h-0 flex-col items-center justify-center px-3 text-center">
                <Database aria-hidden size={18} className="mb-1.5 text-text-disabled" />
                <p className="text-xs font-medium text-text-secondary">暂无节点数据</p>
                <p className="mt-1 max-w-xs text-2xs leading-relaxed text-text-tertiary">
                    配置 OneBot 连接并产生流量后显示
                </p>
            </div>
        );
    }

    return (
        <div className="h-full min-h-0 overflow-auto rounded-sm bg-field ring-1 ring-border-subtle">
            <table className="w-full border-collapse text-left text-2xs">
                <thead className="sticky top-0 z-[1] bg-field text-text-tertiary">
                    <tr className="border-b border-border-subtle">
                        <th className="px-2.5 py-1.5 font-medium">节点</th>
                        <th className="px-2.5 py-1.5 font-medium">类型</th>
                        <th className="px-2.5 py-1.5 font-medium tabular-nums">出/入</th>
                        <th className="px-2.5 py-1.5 font-medium tabular-nums">字节</th>
                        <th className="px-2.5 py-1.5 font-medium tabular-nums">错误</th>
                        <th className="px-2.5 py-1.5 font-medium">活动</th>
                    </tr>
                </thead>
                <tbody>
                    {nodes.map((n) => {
                        const errors = Number(n.errors ?? 0);
                        return (
                            <tr
                                key={`${n.name}-${n.kind}`}
                                className="border-b border-border-subtle/70 last:border-0 hover:bg-inset/35"
                            >
                                <td
                                    className="max-w-[8rem] truncate px-2.5 py-1.5 font-medium text-text"
                                    title={n.name || undefined}
                                >
                                    {n.name || '—'}
                                </td>
                                <td className="whitespace-nowrap px-2.5 py-1.5 text-text-secondary">
                                    {networkNodeKindLabel(n.kind)}
                                </td>
                                <td className="whitespace-nowrap px-2.5 py-1.5 font-mono tabular-nums text-text-secondary">
                                    {formatCompactCount(Number(n.events_out ?? 0))} /{' '}
                                    {formatCompactCount(Number(n.actions_in ?? 0))}
                                </td>
                                <td className="whitespace-nowrap px-2.5 py-1.5 font-mono tabular-nums text-text-secondary">
                                    {formatBytes(Number(n.bytes_out ?? 0))} /{' '}
                                    {formatBytes(Number(n.bytes_in ?? 0))}
                                </td>
                                <td
                                    className={cn(
                                        'px-2.5 py-1.5 font-mono tabular-nums',
                                        errors > 0
                                            ? 'font-semibold text-danger'
                                            : 'text-text-secondary',
                                    )}
                                >
                                    {formatCompactCount(errors)}
                                </td>
                                <td className="whitespace-nowrap px-2.5 py-1.5 text-text-tertiary">
                                    {formatActivity(
                                        n.last_activity_at_ms != null
                                            ? Number(n.last_activity_at_ms)
                                            : null,
                                    )}
                                </td>
                            </tr>
                        );
                    })}
                </tbody>
            </table>
        </div>
    );
}
