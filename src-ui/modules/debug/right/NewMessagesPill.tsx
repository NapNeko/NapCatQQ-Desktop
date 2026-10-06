// 时间线底部居中的小胶囊：往上翻着的时候来了新条目 →「↓ N 条新消息」（brand 实底）；
// 没有新的、但离底部超过一屏 →「↓ 回到最新」（素色）。暂停显示时换成「已暂停」条，点了继续。
// 出现时从下面浮上来，只动 transform / opacity。

import { useLayoutEffect, useRef } from 'react';
import { ArrowDown, Play } from 'lucide-react';
import { cn } from '../../../shared/utils/cn';
import { useMotion } from '../../../hooks/preferences/useMotion';
import { cssEase } from '../../../core/design/cssEase';
import { countFormat } from '../../../core/domain/debug/chatFormat';

function useRiseIn<T extends HTMLElement>(dep: string) {
    const ref = useRef<T>(null);
    const m = useMotion();
    useLayoutEffect(() => {
        const el = ref.current;
        if (!el || !m.enabled || typeof el.animate !== 'function') return;
        const anim = el.animate(
            [
                // 居中用的是 Tailwind 的 translate 属性，和 transform 分开叠加，这里只管竖直方向
                { opacity: 0, transform: 'translateY(8px)' },
                { opacity: 1, transform: 'none' },
            ],
            { duration: m.duration('fast') * 1000, easing: cssEase(m.ease.enter) },
        );
        return () => anim.cancel();
        // 只在出现 / 换了样子时播
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [dep]);
    return ref;
}

export function NewMessagesPill({
    count,
    away,
    onClick,
}: {
    count: number;
    away: boolean;
    onClick: () => void;
}) {
    const visible = count > 0 || away;
    if (!visible) return null;
    return <PillButton count={count} onClick={onClick} />;
}

function PillButton({ count, onClick }: { count: number; onClick: () => void }) {
    const ref = useRiseIn<HTMLButtonElement>(count > 0 ? 'new' : 'away');
    const label = count > 0 ? `${countFormat.format(count)} 条新消息` : '回到最新';
    return (
        <button
            ref={ref}
            type="button"
            onClick={onClick}
            aria-label={count > 0 ? `有 ${count} 条新消息，回到底部` : '回到最新'}
            className={cn(
                'absolute bottom-3 left-1/2 z-20 inline-flex -translate-x-1/2 items-center gap-1 rounded-pill px-3 py-1 text-2xs font-medium shadow-popover transition-colors',
                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-2 focus-visible:ring-offset-canvas',
                count > 0
                    ? 'bg-brand text-white hover:bg-brand-hover'
                    : 'bg-elevated text-text-secondary ring-1 ring-border-subtle hover:text-text',
            )}
        >
            <ArrowDown size={12} strokeWidth={2.4} aria-hidden />
            <span className="tabular-nums">{label}</span>
        </button>
    );
}

/** 暂停显示时的底部条：说明为什么不动了，期间到了多少条，一点就继续 */
export function PausedPill({ pending, onResume }: { pending: number; onResume: () => void }) {
    const ref = useRiseIn<HTMLButtonElement>('paused');
    return (
        <button
            ref={ref}
            type="button"
            onClick={onResume}
            className={cn(
                'absolute bottom-3 left-1/2 z-20 inline-flex max-w-[calc(100%-24px)] -translate-x-1/2 items-center gap-1.5 rounded-pill bg-text px-3 py-1 text-2xs font-medium text-canvas shadow-popover',
                'transition-opacity hover:opacity-90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-2 focus-visible:ring-offset-canvas',
            )}
        >
            <Play size={11} strokeWidth={2.6} aria-hidden className="shrink-0" />
            <span className="truncate tabular-nums">
                已暂停显示{pending > 0 ? ` · 期间到了 ${countFormat.format(pending)} 条` : ''} ·
                继续
            </span>
        </button>
    );
}
