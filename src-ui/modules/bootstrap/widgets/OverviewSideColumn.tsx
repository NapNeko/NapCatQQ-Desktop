// 概览副列：监控关闭时的指挥台 + 一眼三项 + 底注；开启时的双曲线区。

import {
    Activity,
    AlertTriangle,
    Bot,
    ChevronRight,
    Cpu,
    Gauge,
    HardDrive,
    Settings,
} from 'lucide-react';
import { Card, Button } from '../../../shared/ui';
import type { AppRoute } from '../../../shared/components/next/Sidebar';
import type { BotActorSnapshot } from '../../../core/ipc/types';
import type { BotConfig } from '../../../core/ipc/generated/domain/BotConfig';
import type { BootstrapSnapshot } from '../../../core/ipc/types';
import type { LocalVersionSnapshot } from '../../../core/ipc/types';
import {
    computeBotFleetStats,
    glanceSelfCheck,
    listActionableBots,
    countKernelUpdates,
} from '../../../core/domain/overview/glance';
import { botStateBadge } from '../../../core/domain/bot/status';
import type { ReleaseSnapshotView } from '../../../core/domain/release/normalize';
import { OccupancyChart } from './OccupancyChart';
import type { ResourceUsage } from '../../../hooks/diagnostics/useResourceMonitor';

export interface OverviewNavigate {
    (route: AppRoute): void;
}

// ─── 监控 OFF ─────────────────────────────────────────────────────────────

export function OverviewCommandColumn({
    snapshots,
    configs,
    bootstrap,
    localVersions,
    releases,
    onNavigate,
}: {
    snapshots: BotActorSnapshot[];
    configs: Record<string, BotConfig | null>;
    bootstrap: BootstrapSnapshot | null | undefined;
    localVersions: LocalVersionSnapshot;
    releases: ReleaseSnapshotView;
    onNavigate: OverviewNavigate;
}) {
    const stats = computeBotFleetStats(snapshots);
    const actionable = listActionableBots(snapshots);
    const runningList = snapshots.filter((s) => s.state === 'running').slice(0, 4);

    return (
        <div className="flex min-h-0 flex-1 flex-col gap-3">
            <BotCommandCenterCard
                stats={stats}
                actionable={actionable}
                runningList={runningList}
                configs={configs}
                onNavigate={onNavigate}
            />
            <HealthGlanceCard
                stats={stats}
                bootstrap={bootstrap}
                localVersions={localVersions}
                releases={releases}
            />
            <MonitoringDisabledFooter onNavigate={onNavigate} />
        </div>
    );
}

function BotCommandCenterCard({
    stats,
    actionable,
    runningList,
    configs,
    onNavigate,
}: {
    stats: ReturnType<typeof computeBotFleetStats>;
    actionable: ReturnType<typeof listActionableBots>;
    runningList: BotActorSnapshot[];
    configs: Record<string, BotConfig | null>;
    onNavigate: OverviewNavigate;
}) {
    const subtitle =
        stats.total === 0
            ? '尚未创建机器人'
            : stats.crashed > 0 || stats.pendingRestart > 0
              ? `崩溃 ${stats.crashed} · 待重启 ${stats.pendingRestart}`
              : stats.running > 0
                ? `${stats.running} 个运行中`
                : '全部已停止';

    return (
        <Card padding="md" className="flex min-h-0 flex-1 flex-col">
            <div className="mb-3 flex shrink-0 items-start justify-between gap-2">
                <div className="flex items-center gap-2">
                    <div className="grid h-9 w-9 place-items-center rounded-md bg-brand-soft text-brand">
                        <Bot size={18} strokeWidth={1.75} />
                    </div>
                    <div>
                        <h3 className="font-display text-[15px] font-semibold text-text">
                            运行指挥
                        </h3>
                        <p className="text-[12px] text-text-tertiary">{subtitle}</p>
                    </div>
                </div>
                <button
                    type="button"
                    onClick={() => onNavigate('bots')}
                    className="flex shrink-0 items-center gap-0.5 text-[12px] font-medium text-brand hover:underline"
                >
                    机器人
                    <ChevronRight size={14} />
                </button>
            </div>

            <div className="mb-3 shrink-0">
                <p className="font-mono text-3xl font-semibold tabular-nums text-text">
                    {stats.running}
                    <span className="text-lg font-normal text-text-tertiary">
                        {' '}
                        / {stats.total}
                    </span>
                </p>
                <p className="sr-only">
                    {stats.running} 个运行中，共 {stats.total} 个实例
                </p>
                <p className="text-[11.5px] text-text-tertiary">运行中 / 总数</p>
            </div>

            {stats.total === 0 ? (
                <div className="flex flex-1 flex-col items-center justify-center gap-3 py-6 text-center">
                    <p className="text-[13px] text-text-secondary">
                        添加第一个机器人开始管理实例
                    </p>
                    <Button variant="primary" size="sm" onClick={() => onNavigate('bots')}>
                        去机器人页
                    </Button>
                </div>
            ) : (
                <div className="min-h-0 flex-1 space-y-3 overflow-y-auto pr-0.5">
                    {actionable.length > 0 && (
                        <section aria-labelledby="overview-actionable-heading">
                            <h4
                                id="overview-actionable-heading"
                                className="mb-1.5 text-[11px] font-semibold uppercase tracking-wide text-warning"
                            >
                                需处理
                            </h4>
                            <ul className="space-y-1">
                                {actionable.map((item) => (
                                    <li key={item.botId}>
                                        <button
                                            type="button"
                                            onClick={() => onNavigate('bots')}
                                            className="flex w-full items-center justify-between gap-2 rounded-sm bg-warning/5 px-2.5 py-2 text-left transition-colors hover:bg-warning/10"
                                        >
                                            <span className="truncate text-[12.5px] font-medium text-text">
                                                {displayBotName(item.botId, configs)}
                                            </span>
                                            <span className="shrink-0 text-[11px] text-text-tertiary">
                                                {item.detail}
                                            </span>
                                        </button>
                                    </li>
                                ))}
                            </ul>
                        </section>
                    )}

                    {runningList.length > 0 && (
                        <section aria-labelledby="overview-running-heading">
                            <h4
                                id="overview-running-heading"
                                className="mb-1.5 text-[11px] font-semibold uppercase tracking-wide text-text-tertiary"
                            >
                                运行中
                            </h4>
                            <ul className="space-y-1">
                                {runningList.map((snap) => {
                                    const badge = botStateBadge(snap.state);
                                    return (
                                        <li key={snap.bot_id}>
                                            <button
                                                type="button"
                                                onClick={() => onNavigate('bots')}
                                                className="flex w-full items-center justify-between gap-2 rounded-sm px-2.5 py-2 text-left transition-colors hover:bg-inset/80"
                                            >
                                                <span className="truncate text-[12.5px] font-medium text-text">
                                                    {displayBotName(snap.bot_id, configs)}
                                                </span>
                                                <span className="shrink-0 text-[11px] text-success">
                                                    {badge.label}
                                                </span>
                                            </button>
                                        </li>
                                    );
                                })}
                            </ul>
                        </section>
                    )}
                </div>
            )}
        </Card>
    );
}

function HealthGlanceCard({
    stats,
    bootstrap,
    localVersions,
    releases,
}: {
    stats: ReturnType<typeof computeBotFleetStats>;
    bootstrap: BootstrapSnapshot | null | undefined;
    localVersions: LocalVersionSnapshot;
    releases: ReleaseSnapshotView;
}) {
    const updates = countKernelUpdates(localVersions, releases);
    const selfCheck = glanceSelfCheck(bootstrap);

    return (
        <Card padding="md" className="shrink-0">
            <h3 className="mb-3 font-display text-[13px] font-semibold text-text">
                一眼三项
            </h3>
            <div className="grid grid-cols-3 gap-2">
                <GlanceCell
                    label="运行实例"
                    value={`${stats.running}/${stats.total}`}
                    srSummary={`${stats.running} 个运行中，共 ${stats.total} 个`}
                    tone={
                        stats.crashed > 0
                            ? 'danger'
                            : stats.running > 0
                              ? 'success'
                              : 'neutral'
                    }
                />
                <GlanceCell
                    label="内核更新"
                    value={String(updates)}
                    srSummary={updates === 0 ? '无可用内核更新' : `${updates} 项可更新`}
                    tone={updates > 0 ? 'warning' : 'success'}
                />
                <GlanceCell
                    label="系统自检"
                    value={String(selfCheck.issueCount)}
                    srSummary={selfCheck.label}
                    tone={selfCheck.tone}
                />
            </div>
            <p className="mt-2 text-center text-[11px] text-text-disabled">
                {selfCheck.label}
                {updates > 0 ? ` · ${updates} 项内核可更新` : ''}
            </p>
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

function MonitoringDisabledFooter({ onNavigate }: { onNavigate: OverviewNavigate }) {
    return (
        <div className="flex shrink-0 flex-wrap items-center justify-center gap-1.5 px-1 py-1 text-center text-[11px] text-text-tertiary">
            <Gauge size={12} className="shrink-0 opacity-70" aria-hidden />
            <span>性能监控已关闭，未采集本机 CPU / 内存。</span>
            <button
                type="button"
                onClick={() => onNavigate('settings')}
                className="inline-flex items-center gap-0.5 font-medium text-brand hover:underline"
            >
                <Settings size={11} />
                去设置开启
            </button>
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
                <Activity size={20} className="animate-pulse text-brand" />
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
                icon={Cpu}
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
                icon={HardDrive}
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