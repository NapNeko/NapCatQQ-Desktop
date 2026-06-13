// Bot 列表卡壳：与 ComponentManageCard 同层次（surface + 状态 + 底栏操作）。

import type { ReactNode, RefObject } from 'react';
import { cn } from '../../../../shared/utils/cn';
import { Badge } from '../../../../shared/ui';
import type { BotBadgeSpec } from './botCardPresentation';

const SHELL =
    'group relative isolate flex w-full min-w-0 flex-col overflow-hidden ' +
    'rounded-md border border-border-subtle bg-surface shadow-card ' +
    'transition-[box-shadow,border-color] duration-200 hover:border-border hover:shadow-popover';

export function BotManageCard({
    badges,
    selected,
    batchMode,
    accent,
    compact,
    onRowClick,
    header,
    meta,
    chips,
    footerActions,
    lifecycleBadgeRef,
}: {
    badges: BotBadgeSpec[];
    selected?: boolean;
    batchMode?: boolean;
    accent?: 'brand' | 'danger' | 'none';
    /** 远端主机等信息较少的卡：单行 meta，无 chip 区 */
    compact?: boolean;
    onRowClick?: () => void;
    header: ReactNode;
    meta: ReactNode;
    chips?: ReactNode;
    footerActions: ReactNode;
    lifecycleBadgeRef?: RefObject<HTMLSpanElement>;
}) {
    const showMeta = meta != null && meta !== false;
    const hasChips = chips != null && chips !== false;

    return (
        <article
            role={batchMode ? 'button' : undefined}
            onClick={onRowClick}
            className={cn(
                SHELL,
                batchMode && 'cursor-pointer',
                selected && 'border-brand/40 ring-2 ring-brand/35 bg-brand-soft/20',
                accent === 'brand' && 'ring-1 ring-inset ring-brand/25',
                accent === 'danger' && 'ring-1 ring-inset ring-danger/25',
            )}
        >
            {accent === 'brand' ? (
                <span
                    aria-hidden
                    className="absolute inset-y-0 left-0 w-0.5 bg-brand"
                />
            ) : null}
            {accent === 'danger' ? (
                <span
                    aria-hidden
                    className="absolute inset-y-0 left-0 w-0.5 bg-danger"
                />
            ) : null}

            <div
                className={cn(
                    'flex flex-col gap-2',
                    compact ? 'px-3.5 pb-1.5 pt-2.5' : 'px-3.5 pb-2.5 pt-3',
                )}
            >
                <div className="flex items-start gap-3">{header}</div>

                {badges.length > 0 ? (
                    <div className="flex min-w-0 flex-wrap items-center gap-1.5">
                        {badges.map((b, i) => (
                            <Badge
                                key={`${b.label}-${i}`}
                                ref={i === 0 ? lifecycleBadgeRef : undefined}
                                tone={b.tone}
                                appearance="soft"
                                dot={b.dot}
                                className="shrink-0"
                            >
                                {b.label}
                            </Badge>
                        ))}
                    </div>
                ) : null}

                {showMeta ? (
                    <div className="min-w-0">{meta}</div>
                ) : null}

                {hasChips ? (
                    <div className="flex min-w-0 flex-wrap items-center gap-1.5">
                        {chips}
                    </div>
                ) : null}
            </div>

            <footer
                className={cn(
                    'flex shrink-0 items-center justify-end gap-1 border-t border-border-subtle bg-inset/35',
                    compact ? 'min-h-[2.5rem] px-3 py-1.5' : 'min-h-[2.625rem] px-2.5 py-1.5',
                )}
                onClick={(e) => e.stopPropagation()}
            >
                {footerActions}
            </footer>
        </article>
    );
}