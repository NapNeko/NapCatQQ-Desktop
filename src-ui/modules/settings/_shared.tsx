// 设置页各 Tab 共用的小件：FieldRow（左标题右控件行）+ ThemeSegment（主题三选段控件）。

import type { ReactNode } from 'react';
import { Sun, Moon, MonitorCog } from 'lucide-react';
import type { ThemeMode } from '../../hooks/preferences/preferencesStore';

/// shadcn Settings recipe 的标准行：左 label/description 堆叠 + 右控件，
/// space-between 自动右对齐。行间用底部 border + padding 切，isLast 不画线。
export function FieldRow({
    label,
    description,
    isLast,
    children,
}: {
    label: string;
    description?: ReactNode;
    isLast?: boolean;
    children?: ReactNode;
}) {
    return (
        <div
            className={
                'flex items-center justify-between gap-6 ' +
                (isLast ? '' : 'border-b border-border-subtle pb-6')
            }
        >
            <div className="min-w-0 flex-1 space-y-1">
                <label className="block text-[13.5px] font-medium leading-none text-text">
                    {label}
                </label>
                {description && (
                    <p className="text-[12px] leading-relaxed text-text-tertiary">
                        {description}
                    </p>
                )}
            </div>
            {children && (
                <div className="flex shrink-0 items-center gap-2">{children}</div>
            )}
        </div>
    );
}

export function ThemeSegment({
    value,
    onChange,
}: {
    value: ThemeMode;
    onChange: (next: ThemeMode) => void;
}) {
    const items: ReadonlyArray<{
        value: ThemeMode;
        label: string;
        icon: ReactNode;
    }> = [
        { value: 'auto', label: '系统', icon: <MonitorCog size={13} /> },
        { value: 'light', label: '浅色', icon: <Sun size={13} /> },
        { value: 'dark', label: '暗色', icon: <Moon size={13} /> },
    ];
    return (
        <div className="flex h-7 items-center rounded-md bg-inset p-0.5">
            {items.map((it) => (
                <button
                    key={it.value}
                    type="button"
                    onClick={() => onChange(it.value)}
                    className={
                        'flex h-6 items-center gap-1 rounded-sm px-2.5 text-[12px] font-medium transition-colors ' +
                        (value === it.value
                            ? 'bg-surface text-text shadow-[0_1px_2px_rgba(0,0,0,0.04)]'
                            : 'text-text-tertiary hover:text-text')
                    }
                >
                    {it.icon}
                    <span>{it.label}</span>
                </button>
            ))}
        </div>
    );
}
