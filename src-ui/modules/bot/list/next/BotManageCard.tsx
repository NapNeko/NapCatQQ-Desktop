// Bot 列表卡壳：底栏状态组；带状态柔光强调线与精致仪表盘景深。

import type { ReactNode, RefObject } from 'react';
import { cn } from '../../../../shared/utils/cn';
import { Badge } from '../../../../shared/ui';
import type { BotListCardStatus } from './botCardPresentation';
import type { StatusBadgeSpec } from '../../../../core/domain/bot/bot-status-presentation';

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

    const isRunning = status.lifecycle.tone === 'success';
    const isStarting = accent === 'brand' || status.lifecycle.tone === 'brand';
    const isDanger = accent === 'danger' || status.lifecycle.tone === 'danger';

    return (
        <article
            role={batchMode ? 'button' : undefined}
            onClick={onRowClick}
            className={cn(
                'group relative isolate flex min-h-[154px] h-full w-full min-w-0 flex-col justify-between overflow-hidden',
                'rounded-md border bg-surface shadow-card transition-all duration-200',
                batchMode && 'cursor-pointer',
                selected
                    ? 'border-brand/40 ring-2 ring-brand/35 bg-brand-soft/20 shadow-popover'
                    : isDanger
                      ? 'border-danger/35 hover:border-danger/50 shadow-danger/5'
                      : isStarting
                        ? 'border-brand/35 hover:border-brand/50'
                        : isRunning
                          ? 'border-border-subtle hover:border-success/35 hover:shadow-popover'
                          : 'border-border-subtle hover:border-border hover:shadow-popover',
            )}
        >
            {/* 顶部微柔光状态线 */}
            {isDanger ? (
                <span
                    aria-hidden
                    className="absolute inset-x-0 top-0 h-[2px] bg-gradient-to-r from-danger/80 via-danger/40 to-transparent"
                />
            ) : isStarting ? (
                <span
                    aria-hidden
                    className="absolute inset-x-0 top-0 h-[2px] bg-gradient-to-r from-brand/80 via-brand/40 to-transparent"
                />
            ) : isRunning ? (
                <span
                    aria-hidden
                    className="absolute inset-x-0 top-0 h-[2px] bg-gradient-to-r from-success/60 via-success/30 to-transparent"
                />
            ) : null}

            <div
                className={cn(
                    'flex min-h-0 flex-1 flex-col justify-between gap-2',
                    compact ? 'px-3.5 pb-2 pt-2.5' : 'px-4 pb-2.5 pt-3.5',
                )}
            >
                <div className="flex items-start gap-3">{header}</div>

                {showMeta ? <div className="min-w-0 text-xs leading-snug">{meta}</div> : null}

                {hasChips ? (
                    <div className="min-h-[1.5rem] min-w-0">
                        <div className="flex max-h-[1.5rem] min-w-0 items-center gap-1.5 overflow-hidden">
                            {chips}
                        </div>
                    </div>
                ) : null}

                {showMetaExtra ? (
                    <div className="min-w-0 text-xs leading-snug">{metaExtra}</div>
                ) : null}
            </div>

            <footer
                className={cn(
                    'flex h-11 shrink-0 items-center justify-between gap-2 border-t border-border-subtle bg-inset/30',
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
                <div className="flex shrink-0 items-center justify-end gap-1.5">
                    {footerActions}
                </div>
            </footer>
        </article>
    );
}
