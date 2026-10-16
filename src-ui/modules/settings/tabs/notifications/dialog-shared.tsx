// 通知设置 Dialog 共用：表单项布局

import { type ReactNode } from 'react';

export function DialogField({
    label,
    hint,
    children,
    trailing,
}: {
    label: ReactNode;
    hint?: ReactNode;
    children: ReactNode;
    /** 标签行右侧（如启用开关），与控件垂直节奏分开 */
    trailing?: ReactNode;
}) {
    return (
        <div className="min-w-0 space-y-1.5">
            <div className="flex items-start justify-between gap-3">
                <div className="min-w-0 space-y-0.5">
                    <div className="text-xs font-medium text-text-secondary">{label}</div>
                    {hint ? (
                        <p className="text-[11.5px] leading-relaxed text-text-tertiary">{hint}</p>
                    ) : null}
                </div>
                {trailing ? (
                    <div className="flex shrink-0 items-center pt-0.5">{trailing}</div>
                ) : null}
            </div>
            {children}
        </div>
    );
}
