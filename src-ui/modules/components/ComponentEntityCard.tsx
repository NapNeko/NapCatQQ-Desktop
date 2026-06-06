// 组件管理页实体卡：紧凑排版；双列网格见 componentCardGrid.module.css。

import type { ReactNode } from 'react';
import { cn } from '../../shared/utils/cn';
import gridStyles from './componentCardGrid.module.css';

interface ComponentEntityCardProps {
    children: ReactNode;
    footer?: ReactNode;
    progressOverlay?: ReactNode;
}

const CARD_SHELL =
    'group relative isolate box-border flex h-full w-full max-w-full min-w-0 flex-col gap-1.5 rounded-md ' +
    'bg-elevated px-3 py-2.5 shadow-card ring-1 ring-inset ring-border-subtle ' +
    'transition-all duration-150 hover:bg-elevated/90 hover:shadow-popover';

export const componentCardGridClass = gridStyles.componentCardGrid;

export function ComponentEntityCard({
    children,
    footer,
    progressOverlay,
}: ComponentEntityCardProps) {
    const hasFooter = footer != null && footer !== false;
    return (
        <div className={cn(CARD_SHELL, progressOverlay && 'overflow-hidden')}>
            {children}
            {hasFooter ? (
                <div className="flex min-w-0 shrink-0 flex-wrap items-center justify-end gap-1.5 pt-0.5">
                    {footer}
                </div>
            ) : null}
            {progressOverlay}
        </div>
    );
}

export function ComponentCardBody({
    statusDot,
    titleRow,
    description,
    statusLine,
}: {
    statusDot: ReactNode;
    titleRow: ReactNode;
    description?: ReactNode;
    statusLine: ReactNode;
}) {
    const hasDesc =
        description != null && description !== false && String(description).trim() !== '';

    return (
        <div className="flex min-w-0 gap-2">
            {statusDot}
            <div className="min-w-0 flex-1 space-y-0.5">
                {titleRow}
                {hasDesc ? (
                    <p className="line-clamp-1 text-2xs leading-snug text-text-secondary">
                        {description}
                    </p>
                ) : null}
                <div className={cn('min-w-0', hasDesc ? '' : 'pt-0.5')}>{statusLine}</div>
            </div>
        </div>
    );
}