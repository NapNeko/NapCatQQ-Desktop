// Bot 列表卡壳：与 ComponentManageCard 同层次（surface + 底栏徽标 + 操作区）。

import type { ReactNode, RefObject } from 'react';
import { cn } from '../../../../shared/utils/cn';
import { Badge } from '../../../../shared/ui';
import type { BotBadgeSpec } from './botCardPresentation';

const SHELL =
    'group relative isolate flex h-full w-full min-w-0 flex-col overflow-hidden ' +
    'rounded-md border border-border-subtle bg-surface shadow-card ' +
    'transition-[box-shadow] duration-200 hover:shadow-popover';

export function BotManageCard({
    badges,
    selected,
    batchMode,
    accent,
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
    onRowClick?: () => void;
    header: ReactNode;
    meta: ReactNode;
    chips?: ReactNode;
    footerActions: ReactNode;
    lifecycleBadgeRef?: RefObject<HTMLSpanElement>;
}) {
    return (
        <article
            role={batchMode ? 'button' : undefined}
            onClick={onRowClick}
            className={cn(
                SHELL,
                batchMode && 'cursor-pointer',
                selected && 'ring-2 ring-brand bg-brand-soft/25',
                accent === 'brand' && 'ring-1 ring-inset ring-brand/20',
                accent === 'danger' && 'ring-1 ring-inset ring-danger/25',
            )}
        >
            {accent === 'brand' ? (
                <span aria-hidden className="absolute inset-y-0 left-0 w-0.5 bg-brand" />
            ) : null}
            {accent === 'danger' ? (
                <span aria-hidden className="absolute inset-y-0 left-0 w-0.5 bg-danger" />
            ) : null}

            <div className="flex flex-col gap-2 px-3.5 pb-2 pt-3">
                <div className="flex items-start gap-3">{header}</div>
                <div className="min-h-[1.25rem] min-w-0">{meta}</div>
                {chips ? (
                    <div className="flex flex-wrap items-center gap-1.5">{chips}</div>
                ) : null}
            </div>

            <footer className="flex min-h-[2.75rem] shrink-0 items-center justify-between gap-2 border-t border-border-subtle bg-inset/40 px-3 py-2">
                <div className="flex min-w-0 flex-wrap items-center gap-1.5">
                    {badges.map((b, i) => (
                        <Badge
                            key={b.label}
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
                <div
                    className="flex shrink-0 items-center gap-1"
                    onClick={(e) => e.stopPropagation()}
                >
                    {footerActions}
                </div>
            </footer>
        </article>
    );
}