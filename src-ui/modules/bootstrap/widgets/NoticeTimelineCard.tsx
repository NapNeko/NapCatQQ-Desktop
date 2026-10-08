// 概览「最近通知」卡：时间线列表 + 空态瞌睡铃铛。
// 「刚刚 / N 分钟前」是渲染时算的，没有别的刷新来源时每分钟推一下重渲。

import React, { useEffect, useRef, useState } from 'react';
import {
    AlertTriangle,
    BellOff,
    BellRing,
    type LucideIcon,
    MessageSquare,
    PowerOff,
    ChevronRight,
    ArrowUpRight,
} from 'lucide-react';
import { Card } from '../../../shared/ui';
import { useMotion } from '../../../hooks/preferences/useMotion';
import { animateListChildrenEnterAfterPaint } from '../../../shared/ui/motion/listEnter';
import { useOpenExternal } from '../../../hooks/useOpenExternal';
import type { NoticeItem, NoticeTone } from '../../../core/domain/events/notice-aggregator';
import { formatRelativeNoticeTime } from '../../../core/domain/bootstrap/overviewFormat';
import type { AppRoute } from '../../../shared/components/next/Sidebar';

const TONE_VISUAL: Record<
    NoticeTone,
    { icon: LucideIcon; iconBg: string; iconColor: string; dot: string }
> = {
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

export interface NoticeTimelineCardProps {
    notices: NoticeItem[];
    onNavigate: (route: AppRoute) => void;
    className?: string;
}

export const NoticeTimelineCard: React.FC<NoticeTimelineCardProps> = ({
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
                <h3 className="font-display text-[14.5px] font-semibold text-text">最近通知</h3>
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
    const timeInfo = notice.timestamp ? formatRelativeNoticeTime(notice.timestamp) : null;

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
                                    className={`shrink-0 font-mono text-[10px] tabular-nums ${
                                        timeInfo.isRecent
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
