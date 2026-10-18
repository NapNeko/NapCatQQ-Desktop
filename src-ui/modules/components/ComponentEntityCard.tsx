// 组件管理页实体卡：统一图标槽 + 标题旁状态徽章 + 底栏左元信息右操作。

import type { ReactNode } from 'react';
import { Package } from 'lucide-react';
import { cn } from '../../shared/utils/cn';
import { Badge } from '../../shared/ui';
import gridStyles from './componentCardGrid.module.css';
import type { StatusBadgeSpec } from './componentStatusPresentation';

export const componentCardGridClass = gridStyles.componentCardGrid;

export interface ComponentManageCardProps {
    statusBadge: StatusBadgeSpec;
    title: string;
    /** 标题左侧图标；缺省时回退通用组件图标以保持左缘严格对齐 */
    icon?: ReactNode;
    titleAside?: ReactNode;
    description?: string;
    meta: ReactNode;
    footer: ReactNode;
    progressOverlay?: ReactNode;
    accent?: 'brand' | 'none';
}

const SHELL =
    'group relative isolate flex h-full w-full max-w-full min-w-0 flex-col justify-between overflow-hidden ' +
    'rounded-md border border-border-subtle bg-surface shadow-card ' +
    'transition-[box-shadow,border-color] duration-200 hover:border-border hover:shadow-popover';

export function ComponentManageCard({
    statusBadge,
    title,
    icon,
    titleAside,
    description,
    meta,
    footer,
    progressOverlay,
    accent = 'none',
}: ComponentManageCardProps) {
    const desc = description != null && description.trim() !== '' ? description.trim() : null;

    return (
        <article
            className={cn(
                SHELL,
                gridStyles.componentCard,
                accent === 'brand' && 'ring-1 ring-inset ring-brand/25',
            )}
        >
            {accent === 'brand' ? (
                <span aria-hidden className="absolute inset-y-0 left-0 w-0.5 bg-brand" />
            ) : null}

            <div className="flex min-w-0 items-start gap-3 px-3.5 pb-2.5 pt-3">
                <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md border border-border-subtle/80 bg-field/60 text-text-secondary">
                    {icon ?? <Package size={18} strokeWidth={1.9} />}
                </span>

                <div className="flex min-w-0 flex-1 flex-col gap-1">
                    <div className="flex min-w-0 items-center justify-between gap-2">
                        <div className="flex min-w-0 flex-1 flex-wrap items-center gap-x-2 gap-y-1">
                            <h3
                                className="min-w-0 max-w-full truncate font-display text-[14.5px] font-semibold leading-snug text-text"
                                title={title}
                            >
                                {title}
                            </h3>
                            <Badge
                                tone={statusBadge.tone}
                                appearance="soft"
                                dot={statusBadge.dot}
                                className="max-w-[9rem] shrink-0 truncate text-[11px]"
                            >
                                <span className="truncate" title={statusBadge.label}>
                                    {statusBadge.label}
                                </span>
                            </Badge>
                        </div>
                        {titleAside ? <div className="shrink-0">{titleAside}</div> : null}
                    </div>

                    <p
                        className="truncate text-xs leading-snug text-text-secondary"
                        title={desc ?? undefined}
                    >
                        {desc ?? '系统运行环境组件'}
                    </p>
                </div>
            </div>

            <footer
                className={cn(
                    'flex min-h-[2.375rem] shrink-0 items-center justify-between gap-3 border-t border-border-subtle/75 bg-inset/35 px-3.5 py-1.5',
                    gridStyles.cardFooter,
                )}
            >
                <div className="min-w-0 flex-1">{meta}</div>
                <div
                    className={cn(
                        'flex max-w-full shrink-0 flex-wrap items-center justify-end gap-1.5',
                        gridStyles.cardActions,
                    )}
                >
                    {footer}
                </div>
            </footer>

            {progressOverlay}
        </article>
    );
}
