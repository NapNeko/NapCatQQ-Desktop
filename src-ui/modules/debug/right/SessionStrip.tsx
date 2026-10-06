// 会话条：「全部」+ 各个群 / 私聊，按最后活动时间排，带未读数；横着滚，竖滚轮也能横着滚。
//
// 指针停在条上时顺序先冻住：新消息一来会话就往前跳，正要点的那个会从指针底下溜走。

import { memo, useEffect, useMemo, useRef, useState, type WheelEvent } from 'react';
import { User, Users } from 'lucide-react';
import { cn } from '../../../shared/utils/cn';
import type { ChatSession, SessionKey } from '../../../core/domain/debug/chat';

export interface SessionStripProps {
    sessions: Record<SessionKey, ChatSession>;
    active: SessionKey | 'all';
    onSelect: (key: SessionKey | 'all') => void;
}

function byRecent(a: ChatSession, b: ChatSession): number {
    return b.lastAt - a.lastAt || a.key.localeCompare(b.key);
}

export const SessionStrip = memo(function SessionStrip({
    sessions,
    active,
    onSelect,
}: SessionStripProps) {
    const scrollRef = useRef<HTMLDivElement>(null);
    const [frozenOrder, setFrozenOrder] = useState<SessionKey[] | null>(null);

    const list = useMemo(() => {
        const all = Object.values(sessions);
        if (!frozenOrder) return all.sort(byRecent);
        // 冻住时：已有的按冻住时的顺序，新冒出来的接在后面
        const rank = new Map(frozenOrder.map((k, i) => [k, i]));
        return all.sort(
            (a, b) =>
                (rank.get(a.key) ?? Infinity) - (rank.get(b.key) ?? Infinity) || byRecent(a, b),
        );
    }, [sessions, frozenOrder]);

    // 选中的会话一直留在视野里：点了之后、以及别的会话来了新消息把它往后挤的时候（指针在条上时不动，免得抢着滚）
    const activeIndex = active === 'all' ? -1 : list.findIndex((x) => x.key === active);
    useEffect(() => {
        if (frozenOrder) return;
        const el = scrollRef.current?.querySelector<HTMLElement>('[aria-pressed="true"]');
        el?.scrollIntoView?.({ block: 'nearest', inline: 'nearest' });
    }, [active, activeIndex, frozenOrder]);

    const onWheel = (e: WheelEvent<HTMLDivElement>) => {
        const el = e.currentTarget;
        if (Math.abs(e.deltaY) <= Math.abs(e.deltaX) || el.scrollWidth <= el.clientWidth) return;
        el.scrollLeft += e.deltaY;
    };

    return (
        <div
            ref={scrollRef}
            role="toolbar"
            aria-label="会话"
            onWheel={onWheel}
            onPointerEnter={() => setFrozenOrder(list.map((s) => s.key))}
            onPointerLeave={() => setFrozenOrder(null)}
            className="flex shrink-0 items-center gap-1 overflow-x-auto border-b border-border-subtle/70 px-2 py-1.5 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
        >
            <Chip
                label="全部"
                on={active === 'all'}
                onClick={() => onSelect('all')}
                title="所有会话按时间合在一起，每条标来源"
            />
            {list.map((s) => (
                <SessionChip key={s.key} session={s} on={active === s.key} onSelect={onSelect} />
            ))}
            {list.length === 0 && (
                <span className="px-1 text-2xs text-text-tertiary">
                    收到消息后这里会列出群和私聊
                </span>
            )}
        </div>
    );
});

const SessionChip = memo(function SessionChip({
    session,
    on,
    onSelect,
}: {
    session: ChatSession;
    on: boolean;
    onSelect: (key: SessionKey) => void;
}) {
    const Icon = session.type === 'group' ? Users : User;
    return (
        <Chip
            label={session.name}
            on={on}
            onClick={() => onSelect(session.key)}
            title={`${session.type === 'group' ? '群' : '私聊'} ${session.id}`}
            icon={<Icon size={11} strokeWidth={2.2} aria-hidden className="shrink-0 opacity-70" />}
            unread={session.unread}
        />
    );
});

function Chip({
    label,
    on,
    onClick,
    title,
    icon,
    unread = 0,
}: {
    label: string;
    on: boolean;
    onClick: () => void;
    title?: string;
    icon?: React.ReactNode;
    unread?: number;
}) {
    return (
        <button
            type="button"
            aria-pressed={on}
            title={title}
            onClick={onClick}
            aria-label={unread > 0 ? `${label}，${unread} 条未读` : label}
            className={cn(
                'inline-flex h-6 max-w-[11rem] shrink-0 items-center gap-1 rounded-pill px-2.5 text-[12px] transition-colors',
                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-1 focus-visible:ring-offset-canvas',
                on
                    ? 'bg-text font-medium text-canvas'
                    : 'bg-inset text-text-secondary hover:bg-inset/70 hover:text-text',
            )}
        >
            {icon}
            <span className="truncate">{label}</span>
            {unread > 0 && (
                <span className="ml-0.5 inline-flex h-4 min-w-4 shrink-0 items-center justify-center rounded-pill bg-danger px-1 text-[10px] font-semibold leading-none tabular-nums text-white">
                    {unread > 99 ? '99+' : unread}
                </span>
            )}
        </button>
    );
}
