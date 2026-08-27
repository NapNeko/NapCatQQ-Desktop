import React, { useMemo } from 'react';
import {
    AlertTriangle,
    BellRing,
    type LucideIcon,
    MessageSquare,
    Package,
    PowerOff,
    Server,
    Snowflake,
    ThumbsUp,
    ChevronRight,
    ArrowUpRight,
} from 'lucide-react';
import { Card, Badge } from '../../shared/ui';
import { MotionIcon } from '../../shared/ui/motion';
import { Mascot } from '../../shared/components/next/Mascot';
import { usePreferences } from '../../hooks/preferences/preferencesStore';
import logoPng from '../../assets/logo.png?inline';
import { useBootstrap } from '../../hooks/bootstrap/useBootstrap';
import { useBackendSettings } from '../../hooks/preferences/useBackendSettings';
import { useBotSnapshots } from '../../hooks/bot/useBotSnapshots';
import { useBotConfigsMap } from '../../hooks/bot/useBotConfigsMap';
import { useResourceMonitor } from '../../hooks/diagnostics/useResourceMonitor';
import {
    clampPerformanceMonitorIntervalMs,
    PERFORMANCE_MONITOR_INTERVAL_MS_DEFAULT,
} from '../../core/domain/performance/performanceSettings';
import { useReleases } from '../../hooks/diagnostics/useReleases';
import { useEventStream } from '../../hooks/diagnostics/useEventStream';
import { useServerManager } from '../../hooks/remote/useServerManager';
import { useOpenExternal } from '../../hooks/useOpenExternal';
import {
    buildNotices,
    type NoticeItem,
    type NoticeTone,
} from '../../core/domain/events/notice-aggregator';
import {
    findUpdatesAvailable,
    type UpdateAvailableItem,
} from '../../core/domain/release/normalize';
import {
    OverviewCommandColumn,
    PerformanceChartsSection,
} from './widgets/OverviewSideColumn';
import type { AppRoute } from '../../shared/components/next/Sidebar';

export interface BootstrapPanelNextProps {
    onNavigate?: (route: AppRoute) => void;
}

export const BootstrapPanelNext: React.FC<BootstrapPanelNextProps> = ({ onNavigate }) => {
    const { bootstrap } = useBootstrap();
    const { settings } = useBackendSettings();
    const { snapshot: releases } = useReleases();
    const { events } = useEventStream();
    const { data: snapshots = [] } = useBotSnapshots();
    const configs = useBotConfigsMap(snapshots);

    const monitorEnabled = settings?.performanceMonitorEnabled ?? false;
    const monitorInterval = clampPerformanceMonitorIntervalMs(
        settings?.performanceMonitorIntervalMs ?? PERFORMANCE_MONITOR_INTERVAL_MS_DEFAULT,
    );
    const resource = useResourceMonitor({
        enabled: monitorEnabled,
        intervalMs: monitorInterval,
    });
    const motionEnabled = usePreferences().motionEnabled;

    const navigate = onNavigate ?? (() => { });

    const notices = useMemo(
        () =>
            buildNotices({
                bootstrap,
                releases,
                recentEvents: events,
            }),
        [bootstrap, releases, events],
    );

    const updates = useMemo(() => {
        if (!bootstrap?.local_versions) return [];
        return findUpdatesAvailable(bootstrap.local_versions, releases);
    }, [bootstrap?.local_versions, releases]);

    const { servers } = useServerManager();
    const runningCount = snapshots.filter((s) => s.state === 'running').length;

    return (
        <div className="grid min-h-0 flex-1 grid-cols-12 gap-4 pt-8">
            {/* ─── 主列：≥ 1100px 占 7 列 ─── */}
            <div className="col-span-12 flex min-h-0 flex-col gap-4 [@media(min-width:1100px)]:col-span-7">
                <HelloCard
                    runningCount={runningCount}
                    serverCount={servers.length}
                    onNavigate={navigate}
                />
                <RemoteSummaryCard onNavigate={navigate} />
                <NoticeTimelineCard
                    notices={notices}
                    onNavigate={navigate}
                    className="min-h-0 flex-1"
                />
            </div>

            {/* ─── 副列：≥ 1100px 占 5 列 ─── */}
            <div className="col-span-12 flex min-h-0 flex-col gap-4 [@media(min-width:1100px)]:col-span-5">
                <CoreCardsRow
                    napcatVersion={bootstrap?.local_versions.napcat ?? null}
                    snowlumaVersion={bootstrap?.local_versions.snowluma ?? null}
                    updates={updates}
                    onNavigate={navigate}
                />
                {monitorEnabled ? (
                    <PerformanceChartsSection
                        resource={resource}
                        sampleIntervalMs={monitorInterval}
                        motionEnabled={motionEnabled}
                    />
                ) : (
                    <OverviewCommandColumn
                        snapshots={snapshots}
                        configs={configs}
                        onNavigate={navigate}
                    />
                )}
            </div>
        </div>
    );
};

// ─── HelloCard ───────────────────────────────────────────────────────────

function getGreeting(): { title: string; hint: string } {
    const hour = new Date().getHours();
    if (hour >= 5 && hour < 11) {
        return { title: '早上好 !!', hint: '美好的一天，NapCat 正在守护你的机器人实例。' };
    }
    if (hour >= 11 && hour < 14) {
        return { title: '中午好 !!', hint: '午间时刻，各项机器人服务持续稳定运行中。' };
    }
    if (hour >= 14 && hour < 18) {
        return { title: '下午好 !!', hint: '下午时光，各项服务与连接保持健康平稳。' };
    }
    if (hour >= 18 && hour < 23) {
        return { title: '晚上好 !!', hint: '夜晚安宁，NapCatQQ 持续为你守护消息与连接。' };
    }
    return { title: '夜深了 !!', hint: '夜深人静，后台服务依然全天候全自动守候。' };
}

interface HelloCardProps {
    runningCount: number;
    serverCount: number;
    onNavigate: (route: AppRoute) => void;
}

const HelloCard: React.FC<HelloCardProps> = ({ runningCount, serverCount, onNavigate }) => {
    const showMascot = usePreferences().showMascot;
    const openExternal = useOpenExternal();
    const { title, hint } = useMemo(() => getGreeting(), []);

    return (
        <Card variant="hero" className="relative overflow-visible py-7 px-6 sm:px-7 min-h-[175px]">
            <div className="max-w-[340px] pr-2 sm:pr-0">
                <h1 className="font-display text-[35px] font-extrabold leading-none text-[var(--text-hero-title)]">
                    {title}
                </h1>
                <p className="mt-3 text-[14px] leading-relaxed text-text-secondary">
                    {hint}
                </p>

                {/* 状态与导航内联行：纯文字排版与细致微标 */}
                <div className="mt-5 flex flex-wrap items-center gap-x-3 gap-y-1.5 text-xs text-text-secondary">
                    <button
                        type="button"
                        onClick={() => onNavigate('bots')}
                        className="inline-flex items-center gap-1.5 transition-colors hover:text-text cursor-pointer select-none"
                    >
                        <span
                            className={`h-1.5 w-1.5 rounded-full ${runningCount > 0
                                ? 'bg-success shadow-glow-success'
                                : 'bg-text-disabled'
                                }`}
                        />
                        <span>
                            <strong className="font-mono font-semibold text-text tabular-nums">
                                {runningCount}
                            </strong>{' '}
                            个实例运行中
                        </span>
                    </button>

                    <span className="text-border-subtle select-none" aria-hidden>
                        ·
                    </span>

                    <button
                        type="button"
                        onClick={() => onNavigate('remote')}
                        className="inline-flex items-center gap-1.5 transition-colors hover:text-text cursor-pointer select-none"
                    >
                        <Server size={12} className="text-info opacity-90" />
                        <span>
                            <strong className="font-mono font-semibold text-text tabular-nums">
                                {serverCount}
                            </strong>{' '}
                            台主机
                        </span>
                    </button>

                    <span className="text-border-subtle select-none" aria-hidden>
                        ·
                    </span>

                    <button
                        type="button"
                        onClick={() => openExternal('https://github.com/NapNeko/NapCatQQ-Desktop')}
                        className="inline-flex items-center gap-1 text-[var(--text-hero-accent)] hover:underline cursor-pointer select-none"
                    >
                        <ThumbsUp size={12} strokeWidth={2} />
                        <span>GitHub Star</span>
                    </button>
                </div>
            </div>

            {/* mascot：破圈悬浮 */}
            {showMascot && (
                <div className="pointer-events-none absolute -top-8 right-2 hidden md:block lg:right-6">
                    <Mascot
                        primaryColor="var(--brand-500)"
                        secondaryColor="var(--brand-700)"
                        className="h-[195px] w-[130px] drop-shadow-md [&>svg]:h-full [&>svg]:w-full"
                    />
                </div>
            )}
        </Card>
    );
};

// ─── RemoteSummary 卡 ────────────────────────────────────────────────────

const RemoteSummaryCard: React.FC<{ onNavigate?: (route: AppRoute) => void }> = ({
    onNavigate,
}) => {
    const { servers, isLoading } = useServerManager();
    const count = servers.length;
    const description =
        count === 0
            ? '添加 SSH 档案后，可在组件页向远端一键部署 NapCat。'
            : count === 1
                ? '点击进入管理连接与免密配置。'
                : '点击进入管理各台主机的连接与免密配置。';

    const countBadge = isLoading ? (
        <span className="inline-flex h-5 min-w-[2.5rem] items-center justify-center rounded-full bg-inset/80 px-2 text-[11px] text-text-tertiary">
            …
        </span>
    ) : count === 0 ? (
        <span className="inline-flex h-5 items-center rounded-full bg-inset/80 px-2 text-[11px] font-medium text-text-tertiary">
            未配置
        </span>
    ) : (
        <Badge tone="info" appearance="soft" className="font-mono text-2xs">
            {count} 台
        </Badge>
    );

    return (
        <Card
            padding="md"
            hover="lift"
            className="cursor-pointer transition-shadow hover:shadow-popover"
            onClick={() => onNavigate?.('remote')}
        >
            <div className="flex items-start gap-3">
                <div className="grid h-9 w-9 shrink-0 place-items-center rounded-md border border-info/20 bg-info/10 text-info">
                    <MotionIcon icon={Server} motion="breathe" playEnter={false} size={18} />
                </div>
                <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center justify-between gap-x-2 gap-y-0.5">
                        <div className="flex items-center gap-2">
                            <p className="font-display text-[14.5px] font-semibold text-text">远端主机</p>
                            {countBadge}
                        </div>
                        <span className="text-2xs font-medium text-brand inline-flex items-center gap-0.5">
                            管理
                            <ChevronRight size={12} />
                        </span>
                    </div>
                    <p className="mt-1 text-[12px] leading-snug text-text-tertiary">{description}</p>
                </div>
            </div>
        </Card>
    );
};

// ─── NoticeTimeline 卡 ───────────────────────────────────────────────────

const TONE_VISUAL: Record<NoticeTone, { icon: LucideIcon; iconBg: string; iconColor: string; dot: string }> = {
    info: {
        icon: BellRing,
        iconBg: 'bg-info/10',
        iconColor: 'text-info',
        dot: 'bg-info',
    },
    success: {
        icon: MessageSquare,
        iconBg: 'bg-success-soft',
        iconColor: 'text-success',
        dot: 'bg-success',
    },
    warning: {
        icon: PowerOff,
        iconBg: 'bg-warning/10',
        iconColor: 'text-warning',
        dot: 'bg-warning',
    },
    danger: {
        icon: AlertTriangle,
        iconBg: 'bg-danger/10',
        iconColor: 'text-danger',
        dot: 'bg-danger',
    },
};

interface NoticeTimelineCardProps {
    notices: NoticeItem[];
    onNavigate: (route: AppRoute) => void;
    className?: string;
}

const NoticeTimelineCard: React.FC<NoticeTimelineCardProps> = ({
    notices,
    onNavigate,
    className,
}) => (
    <Card padding="md" className={`flex flex-col ${className ?? ''}`.trim()}>
        <div className="mb-3 flex shrink-0 items-center justify-between">
            <h3 className="font-display text-[14.5px] font-semibold text-text">
                Recent Notices
            </h3>
            <span className="text-[12px] text-text-tertiary">
                {notices.length === 0 ? '一切正常' : `最近 ${notices.length} 条`}
            </span>
        </div>

        {notices.length === 0 ? (
            <NoticeEmptyState />
        ) : (
            <ol className="relative min-h-0 flex-1 space-y-2 overflow-y-auto pl-4 pr-1 scrollbar-hide">
                <span
                    aria-hidden
                    className="absolute left-[5px] top-2 bottom-2 w-px bg-border-subtle"
                />

                {notices.map((notice) => (
                    <NoticeRow key={notice.id} notice={notice} onNavigate={onNavigate} />
                ))}
            </ol>
        )}
    </Card>
);

const NoticeRow: React.FC<{
    notice: NoticeItem;
    onNavigate: (route: AppRoute) => void;
}> = ({ notice, onNavigate }) => {
    const openExternal = useOpenExternal();
    const visual = TONE_VISUAL[notice.tone];
    const Icon = visual.icon;
    const timeInfo = notice.timestamp
        ? formatRelativeNoticeTime(notice.timestamp)
        : null;

    return (
        <li className="relative">
            <span
                aria-hidden
                className={`absolute -left-4 top-3.5 h-2.5 w-2.5 rounded-full ring-2 ring-surface ${visual.dot}`}
            />
            <div className="flex items-center justify-between gap-3 rounded-md bg-field/50 border border-border-subtle/60 px-3 py-2 transition-colors hover:bg-field">
                {/* 左侧：图标 + 标题/时间/详情 */}
                <div className="flex items-center gap-3 min-w-0 flex-1">
                    <div
                        className={`grid h-8 w-8 shrink-0 place-items-center rounded-md border border-border-subtle/30 ${visual.iconBg}`}
                    >
                        <Icon size={15} strokeWidth={1.75} className={visual.iconColor} />
                    </div>
                    <div className="min-w-0 flex-1">
                        <div className="flex items-baseline gap-2">
                            <p className="truncate text-xs font-semibold text-text">
                                {notice.title}
                            </p>
                            {timeInfo && (
                                <span
                                    className={`shrink-0 font-mono text-[10px] tabular-nums ${timeInfo.isRecent
                                        ? 'text-success font-semibold flex items-center gap-1'
                                        : 'text-text-tertiary'
                                        }`}
                                >
                                    {timeInfo.isRecent && (
                                        <span className="h-1.5 w-1.5 rounded-full bg-success animate-pulse" />
                                    )}
                                    {timeInfo.text}
                                </span>
                            )}
                        </div>
                        <p className="mt-0.5 truncate text-[11.5px] text-text-tertiary">
                            {notice.detail}
                        </p>
                    </div>
                </div>

                {/* 右侧：纯图标动作按钮 */}
                <div className="flex items-center gap-1.5 shrink-0">
                    {notice.actionText && notice.actionRoute ? (
                        <button
                            type="button"
                            title={notice.actionText}
                            onClick={() => onNavigate(notice.actionRoute as AppRoute)}
                            className="grid h-7 w-7 place-items-center rounded-md border border-border-subtle/50 text-text-tertiary hover:text-brand hover:border-brand/40 hover:bg-surface transition-colors cursor-pointer select-none"
                        >
                            <ChevronRight size={14} />
                        </button>
                    ) : notice.url ? (
                        <button
                            type="button"
                            title="查看详情"
                            onClick={() => openExternal(notice.url!)}
                            className="grid h-7 w-7 place-items-center rounded-md border border-border-subtle/50 text-text-tertiary hover:text-text hover:border-border hover:bg-surface transition-colors cursor-pointer select-none"
                        >
                            <ArrowUpRight size={13} />
                        </button>
                    ) : null}
                </div>
            </div>
        </li>
    );
};

const NoticeEmptyState: React.FC = () => (
    <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 text-center py-6">
        <Package size={20} strokeWidth={1.75} className="text-text-disabled" />
        <p className="text-xs text-text-tertiary">暂无新通知</p>
        <p className="text-[11px] text-text-disabled">系统状态、更新、事件都会在这里出现</p>
    </div>
);

function formatRelativeNoticeTime(unixSeconds: number): { text: string; isRecent: boolean } {
    const nowSec = Math.floor(Date.now() / 1000);
    const diffSec = Math.max(0, nowSec - unixSeconds);

    if (diffSec < 60) {
        return { text: '刚刚', isRecent: true };
    }
    if (diffSec < 3600) {
        const mins = Math.floor(diffSec / 60);
        return { text: `${mins} 分钟前`, isRecent: mins < 15 };
    }
    if (diffSec < 86400) {
        const d = new Date(unixSeconds * 1000);
        const hh = String(d.getHours()).padStart(2, '0');
        const mm = String(d.getMinutes()).padStart(2, '0');
        return { text: `今天 ${hh}:${mm}`, isRecent: false };
    }
    const d = new Date(unixSeconds * 1000);
    const mm = String(d.getMonth() + 1).padStart(2, '0');
    const dd = String(d.getDate()).padStart(2, '0');
    return { text: `${mm}-${dd}`, isRecent: false };
}

// ─── Core 双卡：NapCat + SnowLuma ────────────────────────────────────────

interface CoreCardsRowProps {
    napcatVersion: string | null;
    snowlumaVersion: string | null;
    updates: UpdateAvailableItem[];
    onNavigate: (route: AppRoute) => void;
}

const CoreCardsRow: React.FC<CoreCardsRowProps> = ({
    napcatVersion,
    snowlumaVersion,
    updates,
    onNavigate,
}) => {
    const napcatUpdate = updates.find((u) => u.project === 'napcat');
    const snowlumaUpdate = updates.find((u) => u.project === 'snowluma');

    return (
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <CoreCard
                kind="napcat"
                label="NapCat"
                version={napcatVersion}
                update={napcatUpdate ?? null}
                onNavigate={onNavigate}
            />
            <CoreCard
                kind="snowluma"
                label="SnowLuma"
                version={snowlumaVersion}
                update={snowlumaUpdate ?? null}
                onNavigate={onNavigate}
            />
        </div>
    );
};

interface CoreCardProps {
    kind: 'napcat' | 'snowluma';
    label: string;
    version: string | null;
    update: UpdateAvailableItem | null;
    onNavigate: (route: AppRoute) => void;
}

const CoreCard: React.FC<CoreCardProps> = ({ kind, label, version, update, onNavigate }) => {
    const installed = version !== null;
    const hasUpdate = installed && update !== null;
    const dotClass = installed ? 'bg-success shadow-glow-success' : 'bg-text-disabled/80';

    return (
        <Card
            padding="md"
            hover="lift"
            onClick={() => onNavigate('components')}
            className="flex items-center gap-3.5 transition-all cursor-pointer hover:shadow-popover"
        >
            <div
                className={`grid h-10 w-10 shrink-0 place-items-center rounded-md border border-border-subtle/40 ${kind === 'napcat' ? 'bg-brand-soft/80' : 'bg-info-soft/80'
                    }`}
            >
                {kind === 'napcat' ? (
                    <img src={logoPng} alt="" className="h-6 w-6 select-none" draggable={false} />
                ) : (
                    <Snowflake size={18} strokeWidth={1.75} className="text-info" />
                )}
            </div>

            <div className="min-w-0 flex-1">
                <div className="flex items-center justify-between gap-1">
                    <div className="flex items-center gap-1.5">
                        <span
                            aria-hidden
                            className={`inline-block h-1.5 w-1.5 shrink-0 rounded-full ${dotClass}`}
                        />
                        <p className="truncate font-display text-sm font-semibold leading-none text-text">
                            {label}
                        </p>
                    </div>
                    {hasUpdate && (
                        <Badge tone="warning" appearance="soft" className="text-[10px] px-1 py-0 font-normal">
                            可更新
                        </Badge>
                    )}
                </div>
                <p
                    className={`mt-1.5 truncate text-[11.5px] tabular-nums ${installed ? 'font-mono text-text-secondary' : 'text-text-tertiary'
                        }`}
                >
                    {installed ? formatVersion(version) : '未安装'}
                </p>
            </div>
        </Card>
    );
};

function formatVersion(raw: string): string {
    return /^[vV]/.test(raw) ? raw : `v${raw}`;
}

export default BootstrapPanelNext;
