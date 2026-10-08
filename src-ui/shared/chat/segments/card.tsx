// 卡片外壳：文件 / 转发 / json 卡片共用的 220px 小卡布局。

import type { ReactNode } from 'react';

export function Card({
    icon,
    title,
    sub,
    children,
}: {
    icon: ReactNode;
    title: ReactNode;
    sub?: ReactNode;
    children?: ReactNode;
}) {
    return (
        <span className="my-0.5 flex w-[220px] max-w-full items-start gap-2 rounded-md border border-border-subtle bg-surface/80 px-2.5 py-2">
            <span className="mt-0.5 shrink-0 text-text-tertiary">{icon}</span>
            <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                <span className="line-clamp-2 break-words text-xs font-medium text-text">
                    {title}
                </span>
                {sub && <span className="truncate text-2xs text-text-tertiary">{sub}</span>}
                {children}
            </span>
        </span>
    );
}
