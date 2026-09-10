// 概览副列：监控关闭时的状态摘要 + 实例运行台；开启时的双曲线区。

import {
    Activity,
    AlertTriangle,
    Bot,
    ChevronRight,
    Cpu,
    HardDrive,
    type LucideIcon,
} from 'lucide-react';
import { Card, Button } from '../../../shared/ui';
import { MotionIcon } from '../../../shared/ui/motion';
import type { ComponentType } from 'react';
import type { LucideProps } from 'lucide-react';
import type { AppRoute } from '../../../shared/components/next/Sidebar';
import type { BotActorSnapshot } from '../../../core/ipc/types';
import type { BotConfig } from '../../../core/ipc/generated/domain/BotConfig';
import {
    computeBotFleetStats,
    listActionableBots,
} from '../../../core/domain/overview/glance';
import { OccupancyChart } from './OccupancyChart';
import type { ResourceUsage } from '../../../hooks/diagnostics/useResourceMonitor';

export interface OverviewNavigate {
    (route: AppRoute): void;
}

const overviewStatBob = (Icon: ComponentType<LucideProps>) =>
    function OverviewStatIcon(props: LucideProps) {
        return (
            <MotionIcon
                icon={Icon}
                motion="bob"
                playEnter={false}
                size={props.size ?? 18}
                strokeWidth={props.strokeWidth ?? 1.75}
                className={props.className}
            />
        );
    };

const CpuChartIcon = overviewStatBob(Cpu);
const RamChartIcon = overviewStatBob(HardDrive);
const WarmingActivityIcon = overviewStatBob(Activity);

// ─── 监控 OFF ─────────────────────────────────────────────────────────────

export function OverviewCommandColumn({
    snapshots,
    configs,
    onNavigate,
}: {
    snapshots: BotActorSnapshot[];
    configs: Record<string, BotConfig | null>;
    onNavigate: OverviewNavigate;
}) {
    return (
        <BotFleetOverviewCard
            snapshots={snapshots}
            configs={configs}
            onNavigate={onNavigate}
        />
    );
}

export function BotFleetOverviewCard({
    snapshots,
    configs,
    onNavigate,
}: {
    snapshots: BotActorSnapshot[];
    configs: Record<string, BotConfig | null>;
    onNavigate: OverviewNavigate;
}) {
    const stats = computeBotFleetStats(snapshots);
    const actionable = listActionableBots(snapshots);
    const runningList = snapshots.filter((s) => s.state === 'running').slice(0, 4);

    const subtitle =
        stats.total === 0
            ? '暂无已注册实例'
            : stats.crashed > 0 || stats.pendingRestart > 0
              ? `异常 ${stats.crashed} · 待重启 ${stats.pendingRestart}`
              : stats.running > 0
                ? `${stats.running} 个实例运行中`
                : '所有实例已停止';

    return (
        <Card padding="md" className="flex flex-col gap-3.5">
            {/* 卡片头部 */}
            <div className="flex shrink-0 items-center justify-between gap-2">
                <div className="flex items-center gap-2.5">
                    <div className="grid h-8 w-8 place-items-center rounded-md bg-brand-soft text-brand">
                        <MotionIcon icon={Bot} motion="bob" playEnter={false} size={16} />
                    </div>
                    <div>
                        <h3 className="font-display text-[14.5px] font-semibold text-text">
                            实例状态
                        </h3>
                        <p className="text-[11.5px] text-text-tertiary">{subtitle}</p>
                    </div>
                </div>
                <button
                    type="button"
                    onClick={() => onNavigate('bots')}
                    className="flex shrink-0 items-center gap-0.5 text-xs font-medium text-brand hover:underline cursor-pointer select-none"
                >
                    管理实例
                    <MotionIcon icon={ChevronRight} motion="nudge" playEnter={false} size={13} />
                </button>
            </div>

            {/* 三列关键指标 */}
            <div className="grid grid-cols-3 gap-2">
                <GlanceCell
                    label="运行中"
                    value={String(stats.running)}
                    srSummary={`${stats.running} 个实例正在运行`}
                    tone={
                        stats.crashed > 0
                            ? 'danger'
                            : stats.running > 0
                              ? 'success'
                              : 'neutral'
                    }
                />
                <GlanceCell
                    label="已停止"
                    value={String(stats.stopped)}
                    srSummary={`共 ${stats.stopped} 个已停止实例`}
                    tone="neutral"
                />
                <GlanceCell
                    label="待处置"
                    value={String(actionable.length)}
                    srSummary={
                        actionable.length === 0
                            ? '无异常或待重启项'
                            : `${actionable.length} 项需进入实例页处理`
                    }
                    tone={
                        stats.crashed > 0
                            ? 'danger'
                            : actionable.length > 0
                              ? 'warning'
                              : 'success'
                    }
                />
            </div>

            {/* 列表内容区 */}
            {stats.total === 0 ? (
                <div className="flex flex-col items-center justify-center gap-2 py-4 text-center rounded-md bg-inset/40 border border-dashed border-border-subtle/80">
                    <p className="text-xs text-text-secondary">
                        尚未创建 Bot 实例
                    </p>
                    <Button variant="primary" size="sm" onClick={() => onNavigate('bots')} className="text-2xs h-7">
                        前往实例页创建
                    </Button>
                </div>
            ) : (
                <div className="space-y-2.5">
                    {/* 待处置列表 */}
                    {actionable.length > 0 && (
                        <section aria-labelledby="overview-actionable-heading" className="space-y-1">
                            <h4
                                id="overview-actionable-heading"
                                className="text-[11px] font-semibold uppercase tracking-wide text-warning"
                            >
                                待处置
                            </h4>
                            <div className="space-y-1">
                                {actionable.map((item) => (
                                    <button
                                        key={item.botId}
                                        type="button"
                                        onClick={() => onNavigate('bots')}
                                        className="flex w-full items-center justify-between gap-2 rounded-md bg-warning/10 border border-warning/20 px-3 py-2 text-left transition-colors hover:bg-warning/15 cursor-pointer"
                                    >
                                        <span className="truncate text-xs font-medium text-text">
                                            {displayBotName(item.botId, configs)}
                                        </span>
                                        <span className="shrink-0 text-2xs text-warning font-medium">
                                            {item.detail}
                                        </span>
                                    </button>
                                ))}
                            </div>
                        </section>
                    )}

                    {/* 运行中实例列表 */}
                    {runningList.length > 0 && (
                        <section aria-labelledby="overview-running-heading" className="space-y-1">
                            <h4
                                id="overview-running-heading"
                                className="text-[11px] font-semibold uppercase tracking-wide text-text-tertiary"
                            >
                                活跃实例
                            </h4>
                            <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
                                {runningList.map((snap) => {
                                    const cfg = configs[snap.bot_id];
                                    const backend = cfg?.bot?.backend_type ?? 'napcat';
                                    return (
                                        <button
                                            key={snap.bot_id}
                                            type="button"
                                            onClick={() => onNavigate('bots')}
                                            className="flex w-full items-center justify-between gap-2 rounded-md bg-field/50 border border-border-subtle/70 px-3 py-2 text-left transition-all hover:bg-field hover:border-border cursor-pointer select-none"
                                        >
                                            <div className="flex items-center gap-2 min-w-0">
                                                <span className="h-2 w-2 shrink-0 rounded-full bg-success shadow-glow-success" />
                                                <div className="min-w-0">
                                                    <p className="truncate text-xs font-semibold text-text">
                                                        {displayBotName(snap.bot_id, configs)}
                                                    </p>
                                                    <p className="font-mono text-[10px] text-text-tertiary truncate">
                                                        {backend === 'snowluma' ? 'SnowLuma' : 'NapCat'} · {snap.bot_id}
                                                    </p>
                                                </div>
                                            </div>
                                            <span className="shrink-0 text-[11px] text-success font-mono font-medium">
                                                运行中
                                            </span>
                                        </button>
                                    );
                                })}
                            </div>
                        </section>
                    )}
                </div>
            )}
        </Card>
    );
}

function GlanceCell({
    label,
    value,
    srSummary,
    tone,
}: {
    label: string;
    value: string;
    srSummary: string;
    tone: 'success' | 'warning' | 'danger' | 'neutral';
}) {
    const dot =
        tone === 'success'
            ? 'bg-success'
            : tone === 'warning'
              ? 'bg-warning'
              : tone === 'danger'
                ? 'bg-danger'
                : 'bg-text-disabled';
    return (
        <div
            className="rounded-md bg-inset/60 px-2 py-2.5 text-center"
            role="status"
            aria-label={`${label}：${srSummary}`}
        >
            <div className="mb-1 flex items-center justify-center gap-1">
                <span aria-hidden className={`h-1.5 w-1.5 rounded-full ${dot}`} />
                <span className="text-[10.5px] text-text-tertiary">{label}</span>
            </div>
            <p className="font-mono text-xl font-semibold tabular-nums text-text">{value}</p>
            <span className="sr-only">{srSummary}</span>
        </div>
    );
}

function displayBotName(botId: string, configs: Record<string, BotConfig | null>): string {
    const cfg = configs[botId];
    const name = cfg?.bot?.name?.trim();
    return name || botId;
}

// ─── 监控 ON ─────────────────────────────────────────────────────────────

export function PerformanceChartsSection({
    resource,
    sampleIntervalMs,
    motionEnabled,
}: {
    resource: ResourceUsage;
    sampleIntervalMs: number;
    motionEnabled: boolean;
}) {
    if (resource.status === 'error') {
        return (
            <Card padding="md" className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 text-center">
                <AlertTriangle size={20} className="text-warning" />
                <p className="text-[13px] font-medium text-text">无法读取系统指标</p>
                <p className="max-w-[240px] text-[12px] text-text-tertiary">
                    {resource.errorMessage ?? '请稍后重试'}
                </p>
            </Card>
        );
    }

    if (resource.status === 'warming' && resource.history.length < 1) {
        return (
            <Card padding="md" className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 text-center">
                <WarmingActivityIcon size={20} className="text-brand" />
                <p className="text-[13px] text-text-secondary">正在获取首个采样…</p>
            </Card>
        );
    }

    const cpuText =
        resource.status === 'ready' ? `${resource.cpu}%` : '…';
    const ramText =
        resource.status === 'ready' ? `${resource.ram}%` : '…';

    return (
        <>
            <OccupancyChart
                title="CPU"
                icon={CpuChartIcon as LucideIcon}
                history={resource.history}
                dataKey="cpu"
                valueText={cpuText}
                accentColor="var(--brand-500)"
                sampleIntervalMs={sampleIntervalMs}
                motionEnabled={motionEnabled}
                className="min-h-0 flex-1"
            />
            <OccupancyChart
                title="RAM"
                icon={RamChartIcon as LucideIcon}
                history={resource.history}
                dataKey="ram"
                valueText={ramText}
                accentColor="var(--accent-500)"
                sampleIntervalMs={sampleIntervalMs}
                motionEnabled={motionEnabled}
                className="min-h-0 flex-1"
            />
        </>
    );
}