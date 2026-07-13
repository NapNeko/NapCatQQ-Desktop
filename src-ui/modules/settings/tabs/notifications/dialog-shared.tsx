// 通知设置 Dialog 共用：表单项布局 + 密钥输入

import { useState, type ReactNode } from 'react';
import { Eye, EyeOff } from 'lucide-react';
import { ActionMotionIcon } from '../../../../shared/ui/motion';
import { cn } from '../../../../shared/utils/cn';

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
                    <div className="text-xs font-medium text-text-secondary">
                        {label}
                    </div>
                    {hint ? (
                        <p className="text-[11.5px] leading-relaxed text-text-tertiary">
                            {hint}
                        </p>
                    ) : null}
                </div>
                {trailing ? (
                    <div className="flex shrink-0 items-center pt-0.5">
                        {trailing}
                    </div>
                ) : null}
            </div>
            {children}
        </div>
    );
}

export function SecretField({
    value,
    onValueChange,
    className,
    placeholder,
    name,
}: {
    value: string;
    onValueChange: (v: string) => void;
    className?: string;
    placeholder?: string;
    name?: string;
}) {
    const [reveal, setReveal] = useState(false);
    return (
        <div className={cn('relative w-full min-w-0', className)}>
            <input
                type={reveal ? 'text' : 'password'}
                name={name}
                autoComplete="off"
                spellCheck={false}
                value={value}
                placeholder={placeholder}
                onChange={(e) => onValueChange(e.target.value)}
                className={cn(
                    'block w-full rounded-sm bg-field py-2 pl-3 pr-9 text-sm text-text',
                    'border border-border-subtle outline-none transition-colors duration-150',
                    'placeholder:text-text-tertiary',
                    'disabled:cursor-not-allowed disabled:bg-inset disabled:text-text-disabled',
                    'focus:border-brand focus:ring-2 focus:ring-brand focus:ring-inset',
                )}
            />
            <button
                type="button"
                onClick={() => setReveal((r) => !r)}
                className="absolute right-1 top-1/2 flex h-7 w-7 -translate-y-1/2 items-center justify-center rounded-sm text-text-tertiary transition-colors hover:bg-inset hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
                aria-label={reveal ? '隐藏密钥' : '显示密钥'}
            >
                {reveal ? (
                    <ActionMotionIcon icon={EyeOff} size={15} />
                ) : (
                    <ActionMotionIcon icon={Eye} size={15} />
                )}
            </button>
        </div>
    );
}
