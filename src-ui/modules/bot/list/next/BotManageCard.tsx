// Bot 列表卡壳：底栏状态组（进程 + 账号 + 告警），对齐全站 StatusBadgeSpec。

import type { ReactNode, RefObject } from 'react';
import { cn } from '../../../../shared/utils/cn';
import { Badge } from '../../../../shared/ui';
import type { BotListCardStatus } from './botCardPresentation';
import type { StatusBadgeSpec } from '../../../../core/domain/bot/bot-status-presentation';

const SHELL =
    'group relative isolate flex h-full w-full min-w-0 flex-col overflow-hidden ' +
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
    meta: ReactNode;
    /** 可选：meta 下方附加行（如运行时指标） */
    metaExtra?: ReactNode;
    chips?: ReactNode;
    footerActions: ReactNode;
    /** 进程徽章动效锚点（状态切换 pop） */
    processBadgeRef?: RefObject<HTMLSpanElement>;
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
                    'flex min-h-0 flex-1 flex-col gap-2',
                    compact ? 'px-3.5 pb-1.5 pt-2.5' : 'px-3.5 pb-2 pt-3',
                )}
            >
                <div className="flex items-start gap-3">{header}</div>

                <div
                    className={cn(
                        'min-h-[1.25rem] min-w-0 text-xs leading-snug',
                        !showMeta && 'select-none',
                    )}
                    aria-hidden={!showMeta}
                >
                    {showMeta ? (
                        meta
                    ) : (
                        <p className="truncate text-transparent" aria-hidden>
                            —
                        </p>
                    )}
                    {metaExtra}
                </div>

                <div className="min-h-[1.625rem] min-w-0">
                    {hasChips ? (
                        <div className="flex max-h-[1.625rem] min-w-0 flex-wrap items-center gap-1.5 overflow-hidden">
                            {chips}
                        </div>
                    ) : (
                        <div className="h-[1.625rem]" aria-hidden />
                    )}
                </div>
            </div>

            <footer
                className={cn(
                    'flex shrink-0 items-center gap-2 border-t border-border-subtle bg-inset/35',
                    compact ? 'min-h-[2.5rem] px-3 py-1.5' : 'min-h-[2.75rem] px-3 py-2',
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