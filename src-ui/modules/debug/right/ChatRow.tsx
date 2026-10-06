// 时间线的一行：按条目种类分到气泡 / 通知 / 请求卡片 / 调用标签……，需要时在上面带一条时间分隔线。
//
// 刚来的条目（贴着底时新追加的）挂上时淡入 + 轻微上移，只动 transform / opacity，走 WAAPI（和栏的展开动画同一套），
// 跟随动画设置，减少动画时不动。行外层由虚拟列表用 translateY 定位，动画只加在里层，互不干扰。

import { memo, useLayoutEffect, useRef } from 'react';
import { useMotion } from '../../../hooks/preferences/useMotion';
import { cssEase } from '../../../core/design/cssEase';
import type { ChatItem } from '../../../core/domain/debug/chat';
import { MessageBubble } from './MessageBubble';
import { SafeBoundary } from './rightParts';
import {
    CallChip,
    DroppedRow,
    GapRow,
    MetaRow,
    NoticeRow,
    ReceiverRow,
    RequestCard,
    TimeSeparator,
    TrimmedNote,
} from './SystemRows';

export type TimelineRow = ChatItem | { kind: 'trimmed'; key: string; count: number };

export interface ChatRowProps {
    row: TimelineRow;
    showTime: boolean;
    continued: boolean;
    selected: boolean;
    showSessionName: boolean;
    /** 这一行是这次提交里刚追加的新条目 */
    enter: boolean;
    /** 之前某批里的新条目、那时还没挂上：挂上时来问一句 */
    takeEnter: (key: string) => boolean;
    /** 从「回复」引用跳过来的：每次跳过来加一，行据此闪一下；0 是不闪 */
    flash: number;
}

export const ChatRow = memo(function ChatRow({
    row,
    showTime,
    continued,
    selected,
    showSessionName,
    enter,
    takeEnter,
    flash,
}: ChatRowProps) {
    const m = useMotion();
    const bodyRef = useRef<HTMLDivElement>(null);
    const flashRef = useRef<HTMLSpanElement>(null);

    useLayoutEffect(() => {
        const pending = takeEnter(row.key);
        const el = bodyRef.current;
        if (!(enter || pending) || !m.enabled || !el || typeof el.animate !== 'function') return;
        const dx = row.kind === 'message' ? (row.direction === 'out' ? 10 : -10) : 0;
        el.animate(
            [
                { opacity: 0, transform: `translate(${dx}px, 8px)` },
                { opacity: 1, transform: 'none' },
            ],
            { duration: m.duration('base') * 1000, easing: cssEase(m.ease.enter) },
        );
        // 只在挂上时决定一次
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    useLayoutEffect(() => {
        const el = flashRef.current;
        if (!flash || !el || typeof el.animate !== 'function') return;
        const anim = el.animate([{ opacity: 0.85 }, { opacity: 0 }], {
            duration: m.enabled ? 1400 : 900,
            easing: 'ease-out',
            fill: 'forwards',
        });
        return () => anim.cancel();
    }, [flash, m.enabled]);

    return (
        <div ref={bodyRef} className="relative isolate">
            <span
                ref={flashRef}
                aria-hidden
                className="pointer-events-none absolute inset-x-1 inset-y-0 -z-10 rounded-md bg-brand-soft opacity-0"
            />
            {showTime && row.kind !== 'trimmed' && <TimeSeparator at={row.at} />}
            <SafeBoundary fallback={<BrokenRow />}>
                <RowBody
                    row={row}
                    continued={continued}
                    selected={selected}
                    showSessionName={showSessionName}
                />
            </SafeBoundary>
        </div>
    );
});

function RowBody({
    row,
    continued,
    selected,
    showSessionName,
}: {
    row: TimelineRow;
    continued: boolean;
    selected: boolean;
    showSessionName: boolean;
}) {
    switch (row.kind) {
        case 'message':
            return (
                <MessageBubble
                    item={row}
                    continued={continued}
                    selected={selected}
                    showSessionName={showSessionName}
                />
            );
        case 'notice':
            return <NoticeRow item={row} showSessionName={showSessionName} />;
        case 'request':
            return <RequestCard item={row} />;
        case 'call':
            return <CallChip item={row} />;
        case 'meta':
            return <MetaRow item={row} />;
        case 'receiver':
            return <ReceiverRow item={row} />;
        case 'gap':
            return <GapRow item={row} />;
        case 'dropped':
            return <DroppedRow item={row} />;
        case 'trimmed':
            return <TrimmedNote count={row.count} />;
    }
}

function BrokenRow() {
    return (
        <div className="px-6 py-1 text-center text-2xs text-text-tertiary">
            [这条事件显示不了，切到「列表」看原始数据]
        </div>
    );
}
