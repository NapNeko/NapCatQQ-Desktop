// 页内的一行提示。全局 InfoBar 是给右上角提示栈用的，带进出场动画，放进表单里会先占一块空白。

import { AlertTriangle, Info } from 'lucide-react';
import { cn } from '../../../../shared/utils/cn';

const TONE = {
    info: { box: 'border-info/20 bg-info/5', icon: 'text-info', Icon: Info },
    warning: { box: 'border-warning/30 bg-warning/5', icon: 'text-warning', Icon: AlertTriangle },
} as const;

export function Notice({
    tone = 'info',
    title,
    children,
}: {
    tone?: keyof typeof TONE;
    title: React.ReactNode;
    children?: React.ReactNode;
}) {
    const t = TONE[tone];
    return (
        <div className={cn('flex items-start gap-2 rounded-md border px-3 py-2.5', t.box)}>
            <t.Icon size={15} className={cn('mt-0.5 shrink-0', t.icon)} />
            <div className="min-w-0 flex-1">
                <p className="text-[13px] leading-relaxed text-text">{title}</p>
                {children && (
                    <p className="mt-0.5 text-xs leading-relaxed text-text-secondary">{children}</p>
                )}
            </div>
        </div>
    );
}
