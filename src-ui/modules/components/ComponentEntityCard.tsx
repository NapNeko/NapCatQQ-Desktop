// 组件管理页实体卡：对齐 ServerCard / ContainerCard 的层次（徽章 + 标题 + 元信息 + 底栏操作）。

import type { ReactNode } from 'react';
import { cn } from '../../shared/utils/cn';
import { Badge } from '../../shared/ui';
import gridStyles from './componentCardGrid.module.css';
import type { StatusBadgeSpec } from './componentStatusPresentation';

export const componentCardGridClass = gridStyles.componentCardGrid;

export interface ComponentManageCardProps {
    statusBadge: StatusBadgeSpec;
    title: string;
    titleAside?: ReactNode;
    description?: string;
    meta: ReactNode;
    footer: ReactNode;
    progressOverlay?: ReactNode;
    accent?: 'brand' | 'none';
}

const SHELL =
    'relative isolate flex h-full w-full max-w-full min-w-0 flex-col overflow-hidden ' +
    'rounded-md border border-border-subtle bg-surface shadow-card ' +
    'transition-[box-shadow] duration-200 hover:shadow-popover';

export function ComponentManageCard({
    statusBadge,
    title,
    titleAside,
    description,
    meta,
    footer,
    progressOverlay,
    accent = 'none',
}: ComponentManageCardProps) {
    const desc =
        description != null && description.trim() !== '' ? description.trim() : null;

    return (
        <article
            className={cn(
                SHELL,
                accent === 'brand' && 'ring-1 ring-inset ring-brand/25',
            )}
        >
            {accent === 'brand' ? (
                <span
                    aria-hidden
                    className="absolute inset-y-0 left-0 w-0.5 bg-brand"
                />
            ) : null}

            <div className="flex min-h-0 flex-1 flex-col gap-2 px-3.5 pb-2 pt-3">
                <div className="flex min-w-0 items-start justify-between gap-2">
                    <h3 className="min-w-0 flex-1 truncate font-display text-base font-semibold leading-snug text-text">
                        {title}
                    </h3>
                    {titleAside ? (
                        <div className="shrink-0">{titleAside}</div>
                    ) : null}
                </div>

                <div className="min-w-0 space-y-1">
                    <p
                        className={cn(
                            'line-clamp-2 min-h-[2.25rem] text-xs leading-snug',
                            desc ? 'text-text-secondary' : 'text-transparent select-none',
                        )}
                        aria-hidden={!desc}
                        title={desc ?? undefined}
                    >
                        {desc ?? '占位'}
                    </p>
                </div>

                <div className="min-h-[1.375rem] min-w-0">{meta}</div>
            </div>

            <footer className="flex min-h-[2.75rem] shrink-0 items-center justify-between gap-2 border-t border-border-subtle bg-inset/40 px-3 py-2">
                <Badge
                    tone={statusBadge.tone}
                    appearance="soft"
                    dot={statusBadge.dot}
                    className="shrink-0 max-w-[50%] truncate"
                >
                    {statusBadge.label}
                </Badge>
                <div className="flex min-w-0 flex-1 flex-wrap items-center justify-end gap-1.5">
                    {footer}
                </div>
            </footer>

            {progressOverlay}
        </article>
    );
}