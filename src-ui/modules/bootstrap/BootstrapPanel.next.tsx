// Overview 页（next）。step 3 收尾：接入真实 hook。
//
// 信息架构（沿用 legacy home_page）：
//   主列 (col-span-7)  HelloCard / RemoteSummary / NoticeTimeline
//   副列 (col-span-5)  CoreCardsRow (NapCat + SnowLuma) / Occupancy(CPU) / Occupancy(RAM)
//
// 数据流：
//   - useBootstrap()        → 自检快照 + data_root + local_versions
//   - useReleases()         → 远端 release 快照（已 normalize 为 view 类型）
//   - useEventStream()      → 最近 100 条 DomainEvent
//   - useResourceMonitor()  → CPU/RAM 24 点历史 + 当前值
//   - buildNotices(...)     → 上面 3 个数据源派生 NoticeItem 列表
//
// 严守 frontend-layering：仅 import hooks / shared/ui / domain 派生 / 自身 widgets，
// 不碰 services / @tauri-apps。
//
// 响应式：≥ 1100px 双列 7:5；< 1100px 单列堆叠。

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
} from 'lucide-react';
import { Card } from '../../shared/ui';
import { Mascot } from '../../shared/components/next/Mascot';
import { usePreferences } from '../../hooks/preferences/preferencesStore';
import logoPng from '../../assets/logo.png';
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
import {
    buildNotices,
    type NoticeItem,
    type NoticeTone,
} from '../../core/domain/events/notice-aggregator';
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

    const navigate = onNavigate ?? (() => {});

    const notices = useMemo(
        () =>
            buildNotices({
                bootstrap,
                releases,
                recentEvents: events,
            }),
        [bootstrap, releases, events],
    );

    return (
        // flex-1 撑满父；min-h-0 让 grid 高度被父限定，不再被内容撑大；
        // pt-8 给 mascot 破圈预留空间
        <div className="grid min-h-0 flex-1 grid-cols-12 gap-4 pt-8">
            {/* ─── 主列：内容区 ≥ 1100px 占 7 列；否则占满 12 列 ─── */}
            <div className="col-span-12 flex min-h-0 flex-col gap-4 [@media(min-width:1100px)]:col-span-7">
                <HelloCard />
                <RemoteSummaryCard onNavigate={navigate} />
                <NoticeTimelineCard notices={notices} className="min-h-0 flex-1" />
            </div>

            {/* ─── 副列：≥ 1100px 占 5 列；否则占满 12 列堆到主列下方 ─── */}
            <div className="col-span-12 flex min-h-0 flex-col gap-4 [@media(min-width:1100px)]:col-span-5">
                <CoreCardsRow
                    napcatVersion={bootstrap?.local_versions.napcat ?? null}
                    snowlumaVersion={bootstrap?.local_versions.snowluma ?? null}
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

const HelloCard: React.FC = () => {
    const showMascot = usePreferences().showMascot;
    return (
        <Card variant="hero" padding="lg" className="relative overflow-visible">
            <div className="max-w-[300px] pr-2 sm:pr-0">
                <h1 className="font-display text-[36px] font-extrabold leading-none text-[var(--text-hero-title)]">
                    Hello !!
                </h1>
                <p className="mt-3 text-[14px] leading-relaxed text-text-secondary">
                    欢迎回到主页 NapCatQQ Desktop 帮你高效管理多个 QQ 机器人实例。
                </p>
                <div className="mt-4 inline-flex items-center gap-2 text-[13.5px] text-[var(--text-hero-accent)]">
                    <ThumbsUp size={14} strokeWidth={2} className="shrink-0" />
                    <span>如果你喜欢，请去 GitHub 给个 Star</span>
                </div>
            </div>

            {/* mascot：bottom-0 贴卡底，头从卡顶溢出 ~32px。
                主题色通过 Mascot 组件运行时替换 SVG 衣服色。
                < md 隐藏避免压文字。
                设置页 prefs.showMascot 关闭后整块隐藏。 */}
            {showMascot && (
                <div className="pointer-events-none absolute -top-8 right-2 hidden md:block lg:right-6">
                    <Mascot
                        primaryColor="var(--brand-500)"
                        secondaryColor="var(--brand-700)"
                        className="h-[200px] w-[133px] drop-shadow-md [&>svg]:h-full [&>svg]:w-full"
                    />
                </div>
            )}
        </Card>
    );
};

// ─── RemoteSummary 卡 ────────────────────────────────────────────────────
//
// 接 useServerManager 拿到 ServerManager 中已保存的服务器档案数量。
// react-query 缓存 key 为 ['servers']，与远端页 / useComponents 共享同一份。

const RemoteSummaryCard: React.FC<{ onNavigate?: (route: AppRoute) => void }> = ({
    onNavigate,
}) => {
    const { servers, isLoading } = useServerManager();
    const count = servers.length;
    const description =
        count === 0
            ? '尚未配置远端主机，点击进入 Remote 页面添加。'
            : `已配置 ${count} 台远端主机，点击进入 Remote 页面管理。`;

    return (
        <Card padding="md" hover="lift" className="cursor-pointer" onClick={() => onNavigate?.('remote')}>
            <div className="flex items-center gap-3">
                <div className="grid h-10 w-10 shrink-0 place-items-center rounded-md bg-info/10 text-info">
                    <Server size={18} strokeWidth={1.75} />
                </div>
                <div className="min-w-0 flex-1">
                    <p className="text-[14px] font-semibold text-text">远端主机集群</p>
                    <p className="mt-0.5 text-[12.5px] text-text-tertiary">{description}</p>
                </div>
                <span className="shrink-0 rounded-pill bg-inset px-2.5 py-0.5 text-[11.5px] font-medium text-text-secondary tabular-nums">
                    {isLoading ? '…' : `${count} hosts`}
                </span>
            </div>
        </Card>
    );
};

// ─── NoticeTimeline 卡 ───────────────────────────────────────────────────
//
// 接 buildNotices 派生结果。tone → icon / iconBg / iconColor 在本组件做
// 视觉映射；domain 层只输出语义 tone。

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
    className?: string;
}

const NoticeTimelineCard: React.FC<NoticeTimelineCardProps> = ({ notices, className }) => (
    <Card padding="md" className={`flex flex-col ${className ?? ''}`.trim()}>
        <div className="mb-3 flex shrink-0 items-center justify-between">
            <h3 className="font-display text-[15px] font-semibold text-text">
                Recent Notices
            </h3>
            <span className="text-[12px] text-text-tertiary">
                {notices.length === 0 ? '一切正常' : `最近 ${notices.length} 条`}
            </span>
        </div>

        {notices.length === 0 ? (
            <NoticeEmptyState />
        ) : (
            <ol className="relative min-h-0 flex-1 space-y-2 overflow-y-auto pl-4 pr-1">
                <span
                    aria-hidden
                    className="absolute left-[5px] top-2 bottom-2 w-px bg-border-subtle"
                />

                {notices.map((notice) => (
                    <NoticeRow key={notice.id} notice={notice} />
                ))}
            </ol>
        )}
    </Card>
);

const NoticeRow: React.FC<{ notice: NoticeItem }> = ({ notice }) => {
    const visual = TONE_VISUAL[notice.tone];
    const Icon = visual.icon;
    const dateText = notice.timestamp
        ? formatShortDate(notice.timestamp)
        : null;

    const Inner = (
        <>
            <span
                aria-hidden
                className={`absolute -left-4 top-3 h-2.5 w-2.5 rounded-full ring-2 ring-surface ${visual.dot}`}
            />
            <div className="flex items-start gap-3 rounded-sm bg-inset/50 px-3 py-2.5 transition-colors hover:bg-inset">
                <div className={`grid h-9 w-9 shrink-0 place-items-center rounded-sm ${visual.iconBg}`}>
                    <Icon size={16} strokeWidth={1.75} className={visual.iconColor} />
                </div>
                <div className="min-w-0 flex-1">
                    <div className="flex items-baseline justify-between gap-2">
                        <p className="truncate text-[13.5px] font-semibold text-text">
                            {notice.title}
                        </p>
                        {dateText && (
                            <span className="shrink-0 font-mono text-[11px] text-text-tertiary tabular-nums">
                                {dateText}
                            </span>
                        )}
                    </div>
                    <p className="mt-0.5 truncate text-[12.5px] text-text-tertiary">
                        {notice.detail}
                    </p>
                </div>
            </div>
        </>
    );

    return notice.url ? (
        <li className="relative">
            <a
                href={notice.url}
                target="_blank"
                rel="noopener noreferrer"
                className="block focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand rounded-sm"
            >
                {Inner}
            </a>
        </li>
    ) : (
        <li className="relative">{Inner}</li>
    );
};

const NoticeEmptyState: React.FC = () => (
    <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 text-center">
        <Package size={20} strokeWidth={1.75} className="text-text-disabled" />
        <p className="text-[13px] text-text-tertiary">暂无新通知</p>
        <p className="text-[11.5px] text-text-disabled">系统状态、更新、事件都会在这里出现</p>
    </div>
);

function formatShortDate(unixSeconds: number): string {
    const d = new Date(unixSeconds * 1000);
    const mm = String(d.getMonth() + 1).padStart(2, '0');
    const dd = String(d.getDate()).padStart(2, '0');
    return `${mm}-${dd}`;
}

// ─── Core 双卡：NapCat + SnowLuma ────────────────────────────────────────
//
// 接入 BootstrapSnapshot.local_versions：napcat / snowluma 字段为 null 表
// 示未安装，UI 显示灰点 + "未安装"，整张卡 opacity 65% 暗示未启用。

interface CoreCardsRowProps {
    napcatVersion: string | null;
    snowlumaVersion: string | null;
}

const CoreCardsRow: React.FC<CoreCardsRowProps> = ({ napcatVersion, snowlumaVersion }) => (
    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <CoreCard
            kind="napcat"
            label="NapCat"
            version={napcatVersion}
        />
        <CoreCard
            kind="snowluma"
            label="SnowLuma"
            version={snowlumaVersion}
        />
    </div>
);

interface CoreCardProps {
    kind: 'napcat' | 'snowluma';
    label: string;
    /** null = 未安装。 */
    version: string | null;
}

const CoreCard: React.FC<CoreCardProps> = ({ kind, label, version }) => {
    const installed = version !== null;
    const dotClass = installed ? 'bg-success shadow-glow-success' : 'bg-text-disabled';

    return (
        <Card
            padding="md"
            className={`flex items-center gap-3.5 transition-opacity ${installed ? '' : 'opacity-65'}`}
        >
            <div
                className={`grid h-11 w-11 shrink-0 place-items-center rounded-md ${kind === 'napcat' ? 'bg-brand-soft' : 'bg-info-soft'
                    }`}
            >
                {kind === 'napcat' ? (
                    <img src={logoPng} alt="" className="h-7 w-7 select-none" draggable={false} />
                ) : (
                    <Snowflake size={20} strokeWidth={1.75} className="text-info" />
                )}
            </div>

            <div className="min-w-0 flex-1">
                <div className="flex items-center gap-1.5">
                    <span
                        aria-hidden
                        className={`inline-block h-1.5 w-1.5 shrink-0 rounded-full ${dotClass}`}
                    />
                    <p className="truncate font-display text-[15.5px] font-semibold leading-none text-text">
                        {label}
                    </p>
                </div>
                <p
                    className={`mt-1.5 truncate text-[12px] tabular-nums ${installed ? 'font-mono text-text-secondary' : 'text-text-tertiary'
                        }`}
                >
                    {installed ? formatVersion(version!) : '未安装'}
                </p>
            </div>
        </Card>
    );
};

/// 显示版本号时统一加 `v` 前缀（如果用户原始字符串没有的话）。
function formatVersion(raw: string): string {
    return /^[vV]/.test(raw) ? raw : `v${raw}`;
}

export default BootstrapPanelNext;
