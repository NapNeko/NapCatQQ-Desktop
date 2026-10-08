// 指标页卡片骨架子件：Panel 区块容器 / KPI 瓦片

import { type ReactNode } from 'react';
import { cn } from '../../../shared/utils/cn';

export function Panel({
    title,
    description,
    aside,
    children,
    className,
}: {
    title: string;
    description?: string;
    aside?: ReactNode;
    children: ReactNode;
    className?: string;
}) {
    return (
        <section
            className={cn(
                'flex min-h-0 min-w-0 flex-col overflow-hidden rounded-md bg-elevated/40 ring-1 ring-border-subtle',
                className,
            )}
        >
            <div className="flex shrink-0 items-start justify-between gap-2 border-b border-border-subtle/70 px-3 py-2.5">
                <div className="min-w-0">
                    <div className="flex items-center gap-2">
                        <span
                            aria-hidden
                            className="h-3.5 w-0.5 shrink-0 rounded-full bg-brand/55"
                        />
                        <h2 className="truncate text-[13px] font-semibold text-text">{title}</h2>
                    </div>
                    {description ? (
                        <p className="mt-1 pl-2.5 text-2xs text-text-tertiary">{description}</p>
                    ) : null}
                </div>
                {aside ? <div className="shrink-0">{aside}</div> : null}
            </div>
            <div className="min-h-0 flex-1 overflow-hidden p-3">{children}</div>
        </section>
    );
}

export function KpiTile({
    label,
    value,
    tone = 'brand',
    hint,
}: {
    label: string;
    value: string;
    tone?: 'brand' | 'success' | 'danger' | 'neutral' | 'warning';
    hint?: string;
}) {
    return (
        <div
            className="flex h-full min-h-0 min-w-0 flex-col items-center justify-center rounded-sm bg-inset/50 px-2.5 py-2 text-center"
            role="status"
            aria-label={`${label}：${value}${hint ? `，${hint}` : ''}`}
            title={hint ? `${value} · ${hint}` : value}
        >
            <div className="flex items-center justify-center gap-1.5">
                <span
                    aria-hidden
                    className={cn(
                        'h-1.5 w-1.5 shrink-0 rounded-full',
                        tone === 'brand' && 'bg-brand',
                        tone === 'success' && 'bg-success',
                        tone === 'danger' && 'bg-danger',
                        tone === 'warning' && 'bg-warning',
                        tone === 'neutral' && 'bg-text-disabled',
                    )}
                />
                <p className="text-[10.5px] font-medium leading-none text-text-tertiary">{label}</p>
            </div>
            <p
                className={cn(
                    'mt-1.5 max-w-full truncate font-mono text-[clamp(1.05rem,2.4cqi,1.55rem)] font-semibold leading-none tabular-nums',
                    tone === 'danger' ? 'text-danger' : 'text-text',
                )}
            >
                {value}
            </p>
            {hint ? (
                <p className="mt-1 max-w-full truncate text-[10px] leading-none text-text-tertiary">
                    {hint}
                </p>
            ) : null}
        </div>
    );
}
