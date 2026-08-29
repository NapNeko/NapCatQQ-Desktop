// Bot 列表卡壳：底栏状态组；无 chips 时不占行。

import type { ReactNode, RefObject } from 'react';
import { cn } from '../../../../shared/utils/cn';
import { Badge } from '../../../../shared/ui';
import type { BotListCardStatus } from './botCardPresentation';
import type { StatusBadgeSpec } from '../../../../core/domain/bot/bot-status-presentation';

const SHELL =
    'group relative isolate flex h-[148px] min-h-[148px] w-full min-w-0 flex-col justify-between overflow-hidden ' +
    'rounded-md border border-border-subtle bg-surface shadow-card ' +
    'transition-[box-shadow,border-color] duration-200 hover:border-border hover:shadow-popover';

function StatusBadgePill({
    spec,
    badgeRef,
}: {
    spec: StatusBadgeSpec;
    badgeRef?: RefObject<HTMLSpanElement>;
}) {
    return (
        <Badge
            ref={badgeRef}
            tone={spec.tone}
            appearance="soft"
            dot={spec.dot}
            className="max-w-[9.5rem] shrink-0 truncate"
            title={spec.label}
        >
            {spec.label}
        </Badge>
    );
}

export function BotManageCard({
    status,
    selected,
    batchMode,
    accent,
    compact,
    onRowClick,
    header,
    meta,
    metaExtra,
    chips,
    footerActions,
    processBadgeRef,
}: {
    status: BotListCardStatus;
    selected?: boolean;
    batchMode?: boolean;
    accent?: 'brand' | 'danger' | 'none';
    compact?: boolean;
    onRowClick?: () => void;
    header: ReactNode;
    meta?: ReactNode;
    /** 可选：meta 下方附加行（如运行时指标） */
    metaExtra?: ReactNode;
    chips?: ReactNode;
    footerActions: ReactNode;
    /** 进程徽章动效锚点（状态切换 pop） */
    processBadgeRef?: RefObject<HTMLSpanElement>;
}) {
    const showMeta = meta != null && meta !== false;
    const showMetaExtra = metaExtra != null && metaExtra !== false;
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
                    'flex min-h-0 flex-1 flex-col justify-between',
                    compact ? 'px-3.5 pb-2 pt-2.5' : 'px-4 pb-2.5 pt-3.5',
                )}
            >
                <div className="flex items-start gap-3">{header}</div>

                {showMeta || showMetaExtra ? (
                    <div className="min-w-0 text-xs leading-snug">
                        {showMeta ? meta : null}
                        {showMetaExtra ? metaExtra : null}
                    </div>
                ) : null}

                {hasChips ? (
                    <div className="min-h-[1.625rem] min-w-0">
                        <div className="flex max-h-[1.625rem] min-w-0 flex-wrap items-center gap-1.5 overflow-hidden">
                            {chips}
                        </div>
                    </div>
                ) : null}
            </div>

            <footer
                className={cn(
                    'flex h-11 shrink-0 items-center gap-2 border-t border-border-subtle bg-inset/35',
                    compact ? 'px-3.5 py-1' : 'px-4 py-1.5',
                )}
                onClick={(e) => e.stopPropagation()}
            >
                <div className="flex min-w-0 flex-1 items-center gap-1.5 overflow-hidden">
                    <StatusBadgePill spec={status.lifecycle} badgeRef={processBadgeRef} />
                    {status.session ? (
                        <StatusBadgePill spec={status.session} />
                    ) : (
                        <span
                            className="inline-flex h-5 min-w-[4.5rem] shrink-0 rounded-pill border border-transparent px-2 py-0.5"
                            aria-hidden
                        />
                    )}
                    {status.alert ? <StatusBadgePill spec={status.alert} /> : null}
                </div>
                <div className="flex shrink-0 flex-wrap items-center justify-end gap-1">
                    {footerActions}
                </div>
            </footer>
        </article>
    );
}