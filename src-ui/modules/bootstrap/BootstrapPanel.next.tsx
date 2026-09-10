import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import gsap from 'gsap';
import {
    AlertTriangle,
    BellOff,
    BellRing,
    type LucideIcon,
    MessageSquare,
    PowerOff,
    Server,
    Snowflake,
    ThumbsUp,
    ChevronRight,
    ArrowUpRight,
} from 'lucide-react';
import { Card, Badge } from '../../shared/ui';
import { Counter, MotionIcon } from '../../shared/ui/motion';
import { usePreferences } from '../../hooks/preferences/preferencesStore';
import { useMotion, type MotionEnv } from '../../hooks/preferences/useMotion';
import { animateListChildrenEnterAfterPaint } from '../../shared/ui/motion/listEnter';
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
import { useNoticeEvents } from '../../hooks/events/noticeEventStore';
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
    computeBotFleetStats,
    listActionableBots,
    type BotFleetStats,
} from '../../core/domain/overview/glance';
import {
    getDayPhase,
    getGreeting,
    greetingSeed,
    type DayPhase,
} from '../../core/domain/overview/dayPhase';
import type { ServerProfile } from '../../core/ipc/generated/domain/ServerProfile';
import {
    OverviewCommandColumn,
    PerformanceChartsSection,
} from './widgets/OverviewSideColumn';
import { HeroSky } from './widgets/HeroSky';
import { HeroTitle } from './widgets/HeroTitle';
import { HeroMascot, type MascotReaction } from './widgets/HeroMascot';
import type { AppRoute } from '../../shared/components/next/Sidebar';

export interface BootstrapPanelNextProps {
    onNavigate?: (route: AppRoute) => void;
}

export const BootstrapPanelNext: React.FC<BootstrapPanelNextProps> = ({ onNavigate }) => {
    const { bootstrap } = useBootstrap();
    const { settings } = useBackendSettings();
    const { snapshot: releases } = useReleases();
    const events = useNoticeEvents();
    const { data: snapshots = [], isSuccess: fleetReady } = useBotSnapshots();
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

    const { servers, isLoading: serversLoading } = useServerManager();
    const fleet = useMemo(() => computeBotFleetStats(snapshots), [snapshots]);
    // 与副列「待处置」同一口径（含 6 条上限），两处数字不打架。
    const actionableCount = useMemo(() => listActionableBots(snapshots).length, [snapshots]);

    return (
        <div className="grid min-h-0 flex-1 grid-cols-12 gap-4 pt-8">
            {/* ─── 主列：≥ 1100px 占 7 列 ─── */}
            <div className="col-span-12 flex min-h-0 flex-col gap-4 [@media(min-width:1100px)]:col-span-7">
                <HelloCard
                    fleet={fleet}
                    fleetReady={fleetReady}
                    actionableCount={actionableCount}
                    serverCount={servers.length}
                    onNavigate={navigate}
                />
                <RemoteSummaryCard
                    servers={servers}
                    isLoading={serversLoading}
                    onNavigate={navigate}
                />
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

// 一个实例都没有 / 全停着时，时段闲聊不成立，换成实情。
// 异常数不在这里说，状态行的红色 chip 已经负责。
function fleetHint(fleet: BotFleetStats): string | null {
    if (fleet.total === 0) return '还没有实例，去实例页建一个。';
    if (fleet.active === 0) return '实例都停着，没人说话。';
    return null;
}

interface HelloCardProps {
    fleet: BotFleetStats;
    /** 实例快照至少拿到过一次；之前的数字变化不算「发生了事」。 */
    fleetReady: boolean;
    actionableCount: number;
    serverCount: number;
    onNavigate: (route: AppRoute) => void;
}

const HelloCard: React.FC<HelloCardProps> = ({
    fleet,
    fleetReady,
    actionableCount,
    serverCount,
    onNavigate,
}) => {
    const { showMascot } = usePreferences();
    const m = useMotion();
    const openExternal = useOpenExternal();
    const stageRef = useRef<HTMLDivElement>(null);
    const now = useMinuteClock();
    const phase = getDayPhase(now.getHours());
    const { title, hint: greetingHint } = getGreeting(phase, greetingSeed(now));
    const hint = fleetHint(fleet) ?? greetingHint;
    const runningCount = fleet.running;
    const quips = useMemo(() => mascotQuips(fleet, actionableCount), [fleet, actionableCount]);
    const reaction = useFleetReaction(fleet, actionableCount, fleetReady);
    useDayPhaseOnRoot(phase);
    const burstSparks = useSkySparks(stageRef, m);

    return (
        <Card
            ref={stageRef}
            variant="hero"
            className="relative overflow-visible py-7 px-6 sm:px-7 min-h-[212px]"
            onClick={burstSparks}
        >
            <HeroSky
                phase={phase}
                hour={now.getHours()}
                minute={now.getMinutes()}
                motionEnabled={m.enabled}
            />

            <div className="relative z-10 max-w-[340px] pr-2 sm:pr-0">
                <HeroTitle
                    title={title}
                    className="font-display text-[36px] font-extrabold leading-none tracking-tight text-[var(--text-hero-title)]"
                />
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
                                <Counter value={runningCount} />
                            </strong>{' '}
                            个实例运行中
                        </span>
                    </button>

                    {actionableCount > 0 && (
                        <>
                            <span className="text-border-subtle select-none" aria-hidden>
                                ·
                            </span>
                            <button
                                type="button"
                                onClick={() => onNavigate('bots')}
                                className="inline-flex items-center gap-1.5 text-danger transition-colors hover:text-danger/80 cursor-pointer select-none"
                            >
                                <AlertTriangle size={12} strokeWidth={2} />
                                <span>
                                    <strong className="font-mono font-semibold tabular-nums">
                                        <Counter value={actionableCount} />
                                    </strong>{' '}
                                    个异常
                                </span>
                            </button>
                        </>
                    )}

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
                                <Counter value={serverCount} />
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

            {/* mascot：破圈悬浮，站在天色前面 */}
            {showMascot && (
                <HeroMascot
                    stageRef={stageRef}
                    quips={quips}
                    reaction={reaction}
                    className="absolute -top-9 right-2 z-20 hidden md:block lg:right-6"
                />
            )}
        </Card>
    );
};

// 每分钟醒一次：问候语跨时段要换，太阳月亮也要挪。
function useMinuteClock(): Date {
    const [now, setNow] = useState(() => new Date());
    useEffect(() => {
        const id = window.setInterval(() => setNow(new Date()), 60_000);
        return () => window.clearInterval(id);
    }, []);
    return now;
}

// 整页角落柔光跟着 Hello 卡的天色走；离开概览就摘掉。
function useDayPhaseOnRoot(phase: DayPhase): void {
    useEffect(() => {
        const root = document.documentElement;
        root.setAttribute('data-day-phase', phase);
        return () => root.removeAttribute('data-day-phase');
    }, [phase]);
}

function pick<T>(lines: readonly T[]): T {
    return lines[Math.floor(Math.random() * lines.length)];
}

const ALARM_LINES = ['个实例出事了！', '个实例掉线了！', '个实例不对劲！'] as const;
const RECOVERED_LINES = ['都恢复了，虚惊一场。', '好了，接着跑。', '回来了，我就说没事。'] as const;
const ONLINE_LINES = ['上线了，我盯着。', '起来了，交给我。', '连上了，你去忙。'] as const;
const HALTED_LINES = ['全停了，收工？', '都停了，我也歇会儿。'] as const;

// 实例出事 / 恢复 / 从零到有上线 / 全停时，吉祥物主动说一句。首屏拿到数据前的变化不算。
function useFleetReaction(
    fleet: BotFleetStats,
    actionableCount: number,
    ready: boolean,
): MascotReaction | null {
    const [reaction, setReaction] = useState<MascotReaction | null>(null);
    const prevRef = useRef<{ actionable: number; running: number } | null>(null);
    useEffect(() => {
        if (!ready) return;
        const prev = prevRef.current;
        prevRef.current = { actionable: actionableCount, running: fleet.running };
        if (!prev) return;
        if (actionableCount > prev.actionable) {
            setReaction({
                key: Date.now(),
                text: `${actionableCount} ${pick(ALARM_LINES)}`,
                alarm: true,
            });
        } else if (prev.actionable > 0 && actionableCount === 0) {
            setReaction({ key: Date.now(), text: pick(RECOVERED_LINES) });
        } else if (prev.running === 0 && fleet.running > 0) {
            setReaction({ key: Date.now(), text: pick(ONLINE_LINES) });
        } else if (prev.running > 0 && fleet.running === 0) {
            setReaction({ key: Date.now(), text: pick(HALTED_LINES) });
        }
    }, [ready, actionableCount, fleet.running]);
    return reaction;
}

// 点天空（不是按钮）炸出几颗小星星。星星挂在卡片上，跟 HeroSky 用同一套墨色。
function useSkySparks(
    stageRef: React.RefObject<HTMLDivElement>,
    m: MotionEnv,
): (e: React.MouseEvent<HTMLDivElement>) => void {
    return useCallback(
        (e) => {
            const stage = stageRef.current;
            if (!stage || !m.enabled || m.preset.feel.popPeak === 1) return;
            if ((e.target as HTMLElement).closest('button, a, [role="button"]')) return;
            const rect = stage.getBoundingClientRect();
            const x = e.clientX - rect.left;
            const y = e.clientY - rect.top;
            const sky = stage.querySelector('.ndf-hero-sky');
            if (!sky) return;
            const count = m.preset.feel.overshoot ? 8 : 6;
            for (let i = 0; i < count; i += 1) {
                const spark = document.createElement('span');
                spark.className = 'ndf-hero-spark';
                spark.style.left = `${x}px`;
                spark.style.top = `${y}px`;
                sky.appendChild(spark);
                const angle = (Math.PI * 2 * i) / count + (Math.random() - 0.5) * 0.6;
                const dist = 26 + Math.random() * 30;
                gsap.fromTo(
                    spark,
                    { scale: 0.4, autoAlpha: 1 },
                    {
                        x: Math.cos(angle) * dist,
                        y: Math.sin(angle) * dist - 8,
                        scale: 1,
                        autoAlpha: 0,
                        duration: m.duration('slow') * 2.2,
                        ease: 'power2.out',
                        onComplete: () => spark.remove(),
                    },
                );
            }
        },
        [stageRef, m],
    );
}

const FLAVOR_QUIPS = [
    '喵。',
    '别戳啦，怕痒。',
    '消息我盯着，你去忙。',
    '记得喝水。',
    '尾巴不能摸。',
    '手别抖，戳偏了。',
    '这里没有彩蛋。',
    '刚才那下有点重。',
    '日志我看了，没什么好看的。',
    '再戳我就装作没看见。',
    '你很闲吗。',
    '我也想歇会儿。',
    '好，你戳，我数着。',
    '再戳要收费了。',
] as const;

// 每次开页换一个起点，免得台词顺序永远一样。
const FLAVOR_START = Math.floor(Math.random() * FLAVOR_QUIPS.length);

// 戳吉祥物的台词：第一句说实情，后面几句是她自己的话。
function mascotQuips(fleet: BotFleetStats, actionableCount: number): string[] {
    const status =
        actionableCount > 0
            ? `${actionableCount} 个实例不对劲，去看看？`
            : fleet.total === 0
                ? '一个实例都没有，空得慌。'
                : fleet.running > 0
                    ? `${fleet.running} 个都在跑，我闲着。`
                    : '全停着呢，今天不干活？';
    return [
        status,
        ...FLAVOR_QUIPS.slice(FLAVOR_START),
        ...FLAVOR_QUIPS.slice(0, FLAVOR_START),
    ];
}

// ─── RemoteSummary 卡 ────────────────────────────────────────────────────

const REMOTE_NAMES_SHOWN = 3;

function serverDisplayName(p: ServerProfile): string {
    return p.name?.trim() || p.host?.trim() || p.id;
}

const RemoteSummaryCard: React.FC<{
    servers: ServerProfile[];
    isLoading: boolean;
    onNavigate?: (route: AppRoute) => void;
}> = ({ servers, isLoading, onNavigate }) => {
    const count = servers.length;
    const description = useMemo(() => {
        if (count === 0) return '添加 SSH 档案后，可在组件页向远端一键部署 NapCat。';
        const names = servers.slice(0, REMOTE_NAMES_SHOWN).map(serverDisplayName);
        const rest = count - names.length;
        return rest > 0 ? `${names.join(' · ')} +${rest}` : names.join(' · ');
    }, [servers, count]);

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
                    <p className="mt-1 truncate text-[12px] leading-snug text-text-tertiary">{description}</p>
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
}) => {
    // 「刚刚 / N 分钟前」是渲染时算的，没有别的刷新来源时每分钟推一下。
    const hasTimestamps = notices.some((n) => n.timestamp !== undefined);
    const [, bumpClock] = useState(0);
    useEffect(() => {
        if (!hasTimestamps) return;
        const id = window.setInterval(() => bumpClock((n) => n + 1), 60_000);
        return () => window.clearInterval(id);
    }, [hasTimestamps]);

    // 新通知（崩溃 / 掉线）是运行中冒出来的，让它弹进来而不是凭空出现。
    const m = useMotion();
    const listRef = useRef<HTMLOListElement>(null);
    useEffect(() => {
        const el = listRef.current;
        if (!el) return;
        return animateListChildrenEnterAfterPaint(el, notices.length, m);
    }, [notices.length, m]);

    return (
        <Card padding="md" className={`flex flex-col ${className ?? ''}`.trim()}>
            <div className="mb-3 flex shrink-0 items-center justify-between">
                <h3 className="font-display text-[14.5px] font-semibold text-text">
                    最近通知
                </h3>
                <span className="text-[12px] text-text-tertiary">
                    {notices.length === 0 ? '一切正常' : `最近 ${notices.length} 条`}
                </span>
            </div>

            {notices.length === 0 ? (
                <NoticeEmptyState live={m.enabled} />
            ) : (
                <ol
                    ref={listRef}
                    className="relative min-h-0 flex-1 space-y-2 overflow-y-auto pl-4 pr-1 scrollbar-hide"
                >
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
};

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

// 没事发生时铃铛在打瞌睡：两个 z 从铃铛右上角轮流飘走。
const NoticeEmptyState: React.FC<{ live: boolean }> = ({ live }) => (
    <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 text-center py-6">
        <span className="relative inline-block" aria-hidden>
            <BellOff size={20} strokeWidth={1.75} className="text-text-disabled" />
            <span className={`ndf-snore -right-3 -top-1${live ? ' is-live' : ''}`}>z</span>
            <span className={`ndf-snore -right-1 -top-3${live ? ' is-live' : ''}`}>z</span>
        </span>
        <p className="text-xs text-text-tertiary">暂无新通知</p>
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
    const m = useMotion();
    const iconRef = useRef<HTMLDivElement>(null);

    // 摸到卡片时图标动一下：猫耳朵抖一抖，雪花转一圈。
    const nudgeIcon = () => {
        const el = iconRef.current;
        if (!el || !m.enabled || m.preset.feel.popPeak === 1) return;
        gsap.killTweensOf(el);
        if (kind === 'napcat') {
            gsap.fromTo(
                el,
                { rotate: -8 },
                { rotate: 0, duration: 0.7 / Math.max(0.5, m.speed), ease: 'ndf-wiggle' },
            );
        } else {
            gsap.fromTo(
                el,
                { rotate: 0 },
                { rotate: 180, duration: m.duration('slow') * 2, ease: m.ease.release },
            );
        }
    };

    return (
        <Card
            padding="md"
            hover="lift"
            onClick={() => onNavigate('components')}
            onMouseEnter={nudgeIcon}
            className="flex items-center gap-3.5 transition-all cursor-pointer hover:shadow-popover"
        >
            <div
                className={`grid h-10 w-10 shrink-0 place-items-center rounded-md border border-border-subtle/40 ${kind === 'napcat' ? 'bg-brand-soft/80' : 'bg-info-soft/80'
                    }`}
            >
                <div ref={iconRef} className="grid place-items-center" style={{ transformOrigin: '50% 60%' }}>
                    {kind === 'napcat' ? (
                        <img src={logoPng} alt="" className="h-6 w-6 select-none" draggable={false} />
                    ) : (
                        <Snowflake size={18} strokeWidth={1.75} className="text-info" />
                    )}
                </div>
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
