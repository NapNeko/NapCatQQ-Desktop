// 通用 page 占位：5 个真业务 page 还没接时，AppShell 用它撑起骨架，
// 让 layout 的圆角 / padding / 滚动条等行为先稳定下来。
//
// step 3 之后逐个 page 用真组件替换，这个文件留着给 EventPanel / RemoteHostPanel 等还没轮到的页面用。

import React from 'react';
import type { LucideIcon } from 'lucide-react';
import { Card, CardDescription, CardTitle } from '../../ui';

export interface PagePlaceholderProps {
    title: string;
    icon: LucideIcon;
    description: string;
    /** 该 page 还没接的真功能列表，纯文字提示。 */
    pendingItems?: string[];
}

export const PagePlaceholder: React.FC<PagePlaceholderProps> = ({
    title,
    icon: Icon,
    description,
    pendingItems,
}) => {
    return (
        <div className="flex flex-1 flex-col gap-6 pt-4">
            <header className="flex items-end justify-between">
                <div>
                    <p className="text-2xs uppercase tracking-widest text-text-tertiary">{title.toLowerCase()}</p>
                    <h1 className="font-display text-xl font-semibold text-text">{title}</h1>
                    <p className="mt-1 text-sm text-text-secondary">{description}</p>
                </div>
                <Icon size={20} className="text-text-tertiary" />
            </header>

            <Card variant="outlined" padding="lg" className="flex-1">
                <CardTitle>占位中</CardTitle>
                <CardDescription className="mt-1">
                    该 page 将在后续 step 接入真实业务 hook，当前 AppShell 仅做布局验收。
                </CardDescription>
                {pendingItems && pendingItems.length > 0 && (
                    <ul className="mt-4 space-y-1 text-sm text-text-secondary">
                        {pendingItems.map((item, idx) => (
                            <li key={idx}>· {item}</li>
                        ))}
                    </ul>
                )}
            </Card>
        </div>
    );
};

export default PagePlaceholder;
